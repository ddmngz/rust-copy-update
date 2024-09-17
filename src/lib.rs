use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

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

    // never blocks!
    pub fn read(&self) -> RcuReadGuard<'_, T> {
        self.readers.fetch_add(1, Ordering::Acquire);
        RcuReadGuard { lock: self }
    }

    pub fn update_now(&mut self, new: T) {
        // this raw call is safe because we currently have exclusive reference 
        if let Err(value) = unsafe { self.inner.update(new) } {
            self.synchronize();
            if (unsafe { self.inner.update(value) }).is_err(){
                // can't use an unwrap because i don't want to restrict T to need to ipmlement
                // Debug
                panic!("somehow someone else updated while we were updating, shouldn't be allowed!!");
            }
        }
        self.synchronize();
    }

    // async version of update_now
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

    // block until there are no more readers, guarantees that nobody is reading old data so we can
    // update it
    pub fn synchronize(&self) {
        while self.readers.load(Ordering::Relaxed) != 0 {}
        // even if someone reads in between these two atomics, they will get a reference to the new
        // data, so we can reclaim old data
        unsafe{self.inner.reclaim()};
    }

    //async version of synchronize
    pub async fn asynchronize(&self) {
        self.await;
        // safe for same reason as above
        unsafe{self.inner.reclaim()};
    }

    // lock writes to self, now the user needs to synchronize and reclaim, but they can do that at
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
