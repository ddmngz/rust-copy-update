use std::cell::UnsafeCell;
use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

pub struct InnerLock<T: Sync> {
    data: UnsafeCell<T>,
    locked: AtomicBool,
}

impl<T: Sync> InnerLock<T> {
    pub fn lock(&self) -> InnerGuard<T> {
        self.spin();
        InnerGuard { lock: self }
    }

    pub(crate) fn try_lock(&self) -> Option<InnerGuard<T>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| InnerGuard { lock: self }).ok()
    }

    fn spin(&self) {
        while self
            .locked
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

pub struct InnerGuard<'a, T: Sync> {
    lock: &'a InnerLock<T>,
}

impl<T: Sync> Drop for InnerGuard<'_,T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release)
    }
}

impl<'a, T: Sync> AsRef<T> for InnerGuard<'a,T> {
    fn as_ref(&self) -> &'a T {
        unsafe{self.lock.data.get().as_ref().unwrap()}
    }
}
impl<T: Sync> AsMut<T> for InnerGuard<'_, T> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { self.lock.data.get().as_mut().unwrap() }
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
            locked: false.into(),
        }
    }
}
