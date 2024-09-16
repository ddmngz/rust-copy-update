use crate::InnerGuard;
use crate::InnerLock;
use crate::NeedsReclaim;
use crate::Rcu;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;

impl<T: Sync> Rcu<T> {
    // async version, returns the old value
    pub async fn update_later(&self, data: T) -> Result<T, NeedsReclaim> {
        self.await;
        let mut lock = self.async_lock().await;
        self.update_locked(data, &mut lock)?;
        Ok(self.async_reclaim().await.unwrap())
    }

    pub async fn async_reclaim(&self) -> Option<T> {
        let mut data = self.async_lock().await;
        let (data, new_data) = data.as_mut();
        new_data
            .take()
            .map(|new_data| std::mem::replace(data, new_data))
    }

    pub async fn async_lock(&self) -> InnerGuard<(T, Option<T>)> {
        (&self.data).await
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

impl<T: Sync> Future for &InnerLock<T> {
    type Output = InnerGuard<T>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        match self.try_lock() {
            None => Poll::Pending,
            Some(guard) => Poll::Ready(guard),
        }
    }
}

impl<T: Sync> Future for InnerLock<T> {
    type Output = InnerGuard<T>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        match self.try_lock() {
            None => Poll::Pending,
            Some(guard) => Poll::Ready(guard),
        }
    }
}
