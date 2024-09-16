use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

mod r#async;
mod inner;

use inner::InnerGuard;
use inner::InnerLock;
use inner::RcuReadGuard;

pub struct NeedsReclaim();

pub struct Rcu<T: Sync> {
    data: RcuData<T>,
    readers: AtomicU32,
}

impl<T: Sync> Rcu<T> {
    pub fn new(data: T) -> Self {
        Self {
            data: (data, None).into(),
            readers: 0.into(),
        }
    }

    pub fn read(&self) -> RcuReadGuard<'_, T> {
        self.readers.fetch_add(1, Ordering::Acquire);
        let readers = &self.readers;
        RcuReadGuard::new(self.current_data(), readers)
    }

    pub fn synchronize(&self) {
        while self.get_state() != RcuState::Synchronized {}
    }

    // just update our value, we already have the lock
    pub fn update_locked(
        &self,
        data: T,
        lock: &mut InnerGuard<(T, Option<T>)>,
    ) -> Result<(), NeedsReclaim> {
        let new_data = &mut lock.as_mut().1;
        if self.get_state() == RcuState::TwoRep {
            return Err(NeedsReclaim {});
        }
        let _ = std::mem::replace(new_data, Some(data));
        Ok(())
    }

    // synchronize then update, returns the old value
    pub fn update_now(&self, new: T) -> T {
        self.synchronize();
        let mut data = self.data.lock();
        let data = &mut data.as_mut().0;
        std::mem::replace(data, new)
    }

    pub fn reclaim(&self) -> Option<T> {
        let mut data = self.data.lock();
        let (data, new_data) = data.as_mut();
        if self.get_state() == RcuState::Synchronized {
            new_data
                .take()
                .map(|new_data| std::mem::replace(data, new_data))
        } else {
            None
        }
    }

    fn current_data(&self) -> &T {
        &self.data.as_ref().0
    }

    fn next_data(&self) -> &Option<T> {
        &self.data.as_ref().1
    }

    fn get_state(&self) -> RcuState {
        if self.readers.load(Ordering::Acquire) == 0 {
            RcuState::Synchronized
        } else if self.next_data().is_some() {
            RcuState::TwoRep
        } else {
            RcuState::OneRep
        }
    }
}

pub type RcuData<T> = InnerLock<(T, Option<T>)>;

#[derive(PartialEq)]
enum RcuState {
    OneRep,
    TwoRep,
    Synchronized,
}
