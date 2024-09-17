use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;

pub(crate) struct RcuInner<T: Sync> {
    left: UnsafeCell<MaybeUninit<T>>,
    right: UnsafeCell<MaybeUninit<T>>,
    use_left: AtomicBool,
    both_init: AtomicBool,
    writer_locked: AtomicBool,
    cur: AtomicPtr<T>,
}

impl<'a, T: Sync> RcuInner<T> {
    pub(crate) fn new(data: T) -> Self {
        let mut left = MaybeUninit::new(data);
        // doesn't violate aliasing rules because cur points to left when right gets updated and
        // vice versa
        let cur: AtomicPtr<T> = left.as_mut_ptr().into();
        let left = UnsafeCell::new(left);
        Self {
            left,
            right: MaybeUninit::uninit().into(),
            use_left: true.into(),
            both_init: false.into(),
            writer_locked: false.into(),
            cur,
        }
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
    unsafe fn left_mut(&self) -> &mut MaybeUninit<T> {
        unsafe { self.left.get().as_mut().unwrap() }
    }

    #[allow(clippy::mut_from_ref)]
    unsafe fn right_mut(&self) -> &mut MaybeUninit<T> {
        unsafe { self.right.get().as_mut().unwrap() }
    }

    // only safe if we have the writer lock, or otherwise have exclusive access to self
    pub(crate) unsafe fn update(&self, new: T) -> Result<(), T> {
        if self.both_init.load(Ordering::Acquire) {
            Err(new)
        } else {
            let new = MaybeUninit::new(new);
            // if use left is true then we're overwriting right and vice versa
            let ptr = if self.use_left.fetch_not(Ordering::SeqCst) {
                unsafe { self.right_mut() }
            } else {
                unsafe { self.left_mut() }
            };
            *ptr = new;
            // cur is updated and that's fine since we won't be getting a mutable reference of it
            // anymore
            self.cur.store(ptr.as_mut_ptr(), Ordering::Release);
            Ok(())
        }
    }

    // only safe if we know that nobody is reading previous
    pub(crate) unsafe fn reclaim(&self){
        self.both_init.store(false,Ordering::Relaxed);
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
    pub(crate) fn update(&self, new: T) -> Result<(), T> {
        // safe because by definition we have the lock
        unsafe { self.lock.update(new) }
    }
}
