use std::cell::UnsafeCell;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

pub struct InnerLock<T: Sync> {
    data: UnsafeCell<T>,
    inner_lock: AtomicBool,
}

impl<T: Sync> InnerLock<T> {
    pub fn lock(&self) -> InnerGuard<T> {
        self.spin();
        let data = unsafe { self.data.get().as_ref() }.unwrap().into();
        let lock = (&self.inner_lock).into();
        InnerGuard { data, lock }
    }

    pub(crate) fn try_lock(&self) -> Option<InnerGuard<T>> {
        if self
            .inner_lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let data = unsafe { self.data.get().as_ref() }.unwrap().into();
            let lock = (&self.inner_lock).into();
            Some(InnerGuard { data, lock })
        } else {
            None
        }
    }

    fn spin(&self) {
        while self
            .inner_lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {}
    }
}

impl<T: Sync> AsRef<T> for InnerLock<T> {
    fn as_ref(&self) -> &T {
        unsafe { self.data.get().as_ref() }.unwrap()
    }
}

pub struct InnerGuard<T: Sync> {
    data: NonNull<T>,
    lock: NonNull<AtomicBool>,
}

impl<T: Sync> Drop for InnerGuard<T> {
    fn drop(&mut self) {
        unsafe { self.lock.as_ref() }.store(false, Ordering::Release)
    }
}

impl<T: Sync> AsRef<T> for InnerGuard<T> {
    fn as_ref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}
impl<T: Sync> AsMut<T> for InnerGuard<T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { self.data.as_mut() }
    }
}

pub struct RcuReadGuard<'a, T> {
    data: NonNull<T>,
    readers: &'a AtomicU32,
}

impl<'a, T: Sync> RcuReadGuard<'a, T> {
    pub fn new(data: &T, readers: &'a AtomicU32) -> Self {
        Self {
            data: data.into(),
            readers,
        }
    }
}

impl<T> AsRef<T> for RcuReadGuard<'_, T> {
    fn as_ref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T> Drop for RcuReadGuard<'_, T> {
    fn drop(&mut self) {
        self.readers.fetch_sub(1, Ordering::AcqRel);
    }
}

impl<T: Sync> From<T> for InnerLock<T> {
    fn from(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            inner_lock: false.into(),
        }
    }
}
