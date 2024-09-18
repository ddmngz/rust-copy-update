use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::ptr;

pub(crate) struct RcuInner<T: Sync> {
    left: UnsafeCell<T>,
    right: UnsafeCell<MaybeUninit<T>>,
    use_left: AtomicBool,
    writer_locked: AtomicBool,
    cur: AtomicPtr<T>,
    first_write: AtomicBool,
}

impl<'a, T: Sync> RcuInner<T> {
    pub(crate) fn new(data: T) -> Self {
        let cur: AtomicPtr<T> = AtomicPtr::new(std::ptr::NonNull::dangling().as_ptr());
        let left = UnsafeCell::new(data);
        let right = UnsafeCell::new(MaybeUninit::uninit());
        let me = Self {
            left,
            right,
            use_left: true.into(),
            writer_locked: false.into(),
            cur,
            first_write:true.into(),
        };
        // doesn't violate aliasing rules because cur points to left when right gets updated and
        // vice versa
        // we also know that left is initialized since we just initialized it
        //
        //
        //let ptr = std::ptr::addr_of_mut!(me.left);
        let ptr = me.left.get();
        me.cur.store(ptr, Ordering::Acquire);
        me
    }

    pub fn print_layout(&self){
        eprintln!("left is at {:p}, right is at {:p}, cur is at {:p}", &self.left, &self.right, self.cur)
    }

    pub(crate) fn try_lock(&'a self) -> Option<InnerGuard<'a, T>> {
        if self
            .writer_locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(InnerGuard { lock: self })
        } else {
            None
        }
    }

    pub(crate) fn lock(&'a self) -> InnerGuard<'a, T> {
        self.spin();
        InnerGuard { lock: self }
    }

    // safe because cur always points to the newest data
    fn get_cur(&self) -> &T {
        unsafe { self.cur.load(Ordering::SeqCst).as_ref() }.unwrap()
    }

    fn spin(&self) {
        while self
            .writer_locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {}
    }

    #[allow(clippy::mut_from_ref)]
    // only safe if we have exclusive reference (via reader lock)
    unsafe fn left_mut(&self) -> &mut T {
        unsafe { self.left.get().as_mut().unwrap() }
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn right_mut(&self) -> &mut MaybeUninit<T> {
        unsafe { self.right.get().as_mut().unwrap() }
    }

    // only safe if there are no living references to old values, and if we have the writer lock
    pub(crate) unsafe fn update(&self, new: T) {
        // first case: if right is uninitialized, then we have to write through .write to avoid
        // dropping uninitialized data
        if self.first_write.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok(){
            unsafe{self.right_mut()}.write(new);
            // if we were using left before, we need to update right
        }else if self.use_left.fetch_not(Ordering::SeqCst) {
            let right = unsafe { self.right_mut().assume_init_mut() };
            *right = new;
            self.cur.store(right, Ordering::Release);
        } else {
            // otherwise we update left mut
            unsafe { self.left_mut() };
            self.cur.store(self.left_mut(), Ordering::Release);
        };
        // if use left is true then we're overwriting right and vice versa
    }

}

impl<T: Sync> AsRef<T> for RcuInner<T> {
    fn as_ref(&self) -> &T {
        self.get_cur()
    }
}

pub struct InnerGuard<'a, T: Sync> {
    lock: &'a RcuInner<T>,
}

impl<T: Sync> Drop for InnerGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.writer_locked.store(false, Ordering::Release)
    }
}

impl<'a, T: Sync> AsRef<T> for InnerGuard<'a, T> {
    fn as_ref(&self) -> &'a T {
        self.lock.get_cur()
    }
}

impl<'a, T: Sync> InnerGuard<'a, T> {
    pub(crate) fn update(&self, new: T){
        // safe because by definition we have the lock
        unsafe { self.lock.update(new) }
    }
}
