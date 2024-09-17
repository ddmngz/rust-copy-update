use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

mod r#async;
mod inner;

use inner::InnerGuard;
use inner::RcuInner;

pub struct NeedsReclaim();

pub struct Rcu<T: Sync> {
    inner: RcuInner<T>,
    readers: AtomicU32,
}

impl<'a, T: Sync> Rcu<T> {
    pub fn new(data: T) -> Self {
        Self {
            inner: RcuInner::new(data),
            readers: 0.into(),
        }
    }

    pub fn read(&self) -> RcuReadGuard<'_, T> {
        self.readers.fetch_add(1, Ordering::Acquire);
        RcuReadGuard { lock: self }
    }

    pub fn update_now(&mut self, new: T) {
        if let Err(value) = unsafe { self.inner.update(new) } {
            self.synchronize();
            self.update_now(value);
        }
        self.synchronize();
    }

    pub async fn update_later(&self, new: T) {
        let lock = (&self.inner).await;
        if let Err(new) = lock.update(new) {
            drop(lock);
            self.asynchronize().await;
            self.update_later(new).await
        } else {
            self.asynchronize().await;
        }
    }

    pub async fn asynchronize(&self) {
        self.await
    }

    // block until all readers are reading the same data
    fn synchronize(&self) {
        while self.readers.load(Ordering::Relaxed) != 0 {}
    }

    // lock writes to self, now the user needs to synchronize and reclaim, but they can do it at
    // their leisure
    pub fn write(&'a self) -> RcuWriteGuard<'a, T> {
        let inner_lock = self.inner.lock();
        RcuWriteGuard {
            lock: self,
            inner: inner_lock,
        }
    }
}

pub struct RcuWriteGuard<'a, T: Sync> {
    lock: &'a Rcu<T>,
    inner: InnerGuard<'a, T>,
}

impl<'a, T: Sync> RcuWriteGuard<'a, T> {
    pub fn update(&self, new: T) -> Result<(), T> {
        self.inner.update(new)
    }

    pub fn synchronize(&self) {
        self.lock.synchronize();
    }
}

impl<'a, T: Sync> AsRef<T> for RcuWriteGuard<'a, T> {
    fn as_ref(&self) -> &T {
        self.inner.as_ref()
    }
}

pub struct RcuReadGuard<'a, T: Sync> {
    lock: &'a Rcu<T>,
}

impl<T: Sync> AsRef<T> for RcuReadGuard<'_, T> {
    fn as_ref(&self) -> &T {
        self.lock.inner.as_ref()
    }
}

impl<T: Sync> Drop for RcuReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.readers.fetch_sub(1, Ordering::AcqRel);
    }
}

impl<T: Sync> Future for Rcu<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.readers.load(Ordering::Acquire) > 0 {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

// polling on Rcu means waiting until there are no readers
impl<T: Sync> Future for &Rcu<T> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.readers.load(Ordering::Acquire) > 0 {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}

// polling on inner means getting the write lock
impl<'a, T: Sync> Future for &'a RcuInner<T> {
    type Output = InnerGuard<'a, T>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        match self.try_lock() {
            None => Poll::Pending,
            Some(guard) => Poll::Ready(guard),
        }
    }
}
