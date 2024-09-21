//this is so fucked up wth

use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::async_ext::AsyncRcu;
use crate::mut_rcu::MutRcu;
use crate::Rcu;
use crate::RcuWriteGuard;

struct EvilRcu<T: Sync> {
    newest: EvilRcuInner<T>,
    locked: AtomicBool,
}

impl<'a, T: Sync + 'a> Rcu<'a, T> for EvilRcu<T> {
    type ReadGuard = EvilReadGuard<T>;
    type WriteGuard = EvilWriteGuard<'a, T>;
    fn read(&'a self) -> EvilReadGuard<T> {
        let inner = self.newest.get_cloned();
        EvilReadGuard { inner }
    }

    fn write(&'a self) -> EvilWriteGuard<T> {
        self.lock();
        EvilWriteGuard { lock: self }
    }
    fn synchronize(&self) {
        while self.newest.ref_count() > 1 {}
    }
}

impl<'a, T: Sync + 'a> EvilRcu<T> {
    fn new(data: T) -> Self {
        Self {
            newest: EvilRcuInner::new(Arc::new(data)),
            locked: false.into(),
        }
    }

    fn lock(&self) {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {}
    }
}

struct EvilRcuInner<T: Sync>(UnsafeCell<Arc<T>>);

impl<T: Sync> EvilRcuInner<T> {
    fn new(data: Arc<T>) -> Self {
        Self(UnsafeCell::new(data))
    }

    // SAFETY
    // Aligned, Dereferencable, initialized, and NonNull because we only initialize it through Self::new
    // we know there's no existing mutable aliases because there is no interface to get mutable
    // references. (In the docs it says it's okay to mutate through an
    // UnsafeCell)
    fn get_cloned(&self) -> Arc<T> {
        unsafe { self.0.get().as_ref() }.unwrap().clone()
    }

    // SAFETY
    // Aligned, Dereferencable, initialized, and NonNull because we only initialize it through Self::new and update it through this fn
    fn replace(&self, new: Arc<T>) -> Arc<T> {
        unsafe { self.0.get().replace(new) }
    }

    // SAFETY same safety as get_cloned
    fn ref_count(&self) -> usize {
        Arc::strong_count(unsafe { self.0.get().as_ref() }.unwrap())
    }
}

struct EvilReadGuard<T: Sync> {
    inner: Arc<T>,
}

impl<'a, T: Sync + 'a> AsRef<T> for EvilReadGuard<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

struct EvilWriteGuard<'a, T: Sync> {
    lock: &'a EvilRcu<T>,
}

impl<'a, T: Sync + 'a> RcuWriteGuard<'a, T> for EvilWriteGuard<'a, T> {
    fn update_unsynced(&mut self, data: T) {
        self.lock.newest.replace(Arc::new(data));
    }

    fn synchronize(&self) {
        self.lock.synchronize();
    }
}

impl<'a, T: Sync + 'a> Drop for EvilWriteGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release)
    }
}

impl<'a, T: Sync + 'a> MutRcu<'a, T> for EvilRcu<T> {
    fn mut_write(&'a mut self) -> EvilWriteGuard<'a, T> {
        self.locked.store(true, Ordering::Release);
        EvilWriteGuard { lock: self }
    }
}

use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

impl<'a, T: Sync + 'a> Future for &'a EvilRcu<T> {
    type Output = &'a EvilRcu<T>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Poll::Ready(*self)
        } else {
            Poll::Pending
        }
    }
}

impl<T: Sync> Future for &EvilRcuInner<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ref_count() == 1 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl<'a, T: Sync + 'a> AsyncRcu<'a, T> for EvilRcu<T> {
    async fn poll_write(&'a self) -> Self::WriteGuard {
        EvilWriteGuard { lock: self.await }
    }

    async fn poll_synchronize(&self) {
        (&self.newest).await
    }

    async fn poll_update(&self, data: T) {
        let mut lock = self.poll_write().await;
        lock.update_unsynced(data);
        self.poll_synchronize().await;
    }
}
