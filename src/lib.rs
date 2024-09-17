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

impl<'a, T: Sync> Rcu<T> {
    pub fn new(data: T) -> Self {
        Self {
            data: (data, None).into(),
            readers: 0.into(),
        }
    }

    pub fn read(&self) -> RcuReadGuard<'_, T> {
        self.readers.fetch_add(1, Ordering::Acquire);
        let readers = &self.readers;
        let data = self.next_data().as_ref().unwrap_or_else(|| self.current_data());
        RcuReadGuard::new(data, readers)
    }


    // There are 3 stages to writing in the RCU, update, synchronize, and reclaim:
    // - Update provides a new value to the RCU
    // - synchronize ensures that there are no references to the old data
    // - reclaim frees whatever memory the old data was pointing to
    //
    // I provide the raw interfaces to update, synchronize, and reclaim via update_locked,
    // otherwise update_now

    // Spin until there are no readers left, and then free old value
    // Doesn't matter that new readers can show up, all new readers will get new data
    // FIXME in the kernel, the user handles memory and so it's trivial that next_data and data
    // never move, however in this implementation, that's not the case. Maybe use Pin for this?
    // or like two MaybeUninit<T>s with a bool to indicate which one we're
    // currently using? idk that sucks ngl
    // perhaps you clone T and store it twice initially, arbitrarily decide ones the starting one
    // and go from there
    //
    pub fn synchronize(&self) {
        while self.get_state() != RcuState::Synchronized {}
    }

    // just update our value
    pub fn update_locked(
        &self,
        data: T,
        lock: &mut InnerGuard<(T, Option<T>)>,
    ) -> Result<(), NeedsReclaim> {
        let new_data = Self::next_mut(lock);
        if self.get_state() == RcuState::TwoRep {
            return Err(NeedsReclaim {});
        }
        let _ = std::mem::replace(new_data, Some(data));
        Ok(())
    }

    // updates, synchronize, then reclaims, all in order
    // note this blocks twice, first to get the updater lock, and then again to synchronize
    pub fn update_now(&self, new: T) {
        let mut lock = self.data.lock();
        let data = Self::next_mut(&mut lock);
        let _ = std::mem::replace(data, Some(new));
        self.synchronize();
        unsafe {Self::raw_reclaim(&mut lock)};
    }

    pub fn reclaim(&self) -> Option<T> {
        let mut data = self.data.lock();
        if self.get_state() == RcuState::Synchronized {
            unsafe {
                Self::raw_reclaim(&mut data)
            }
        } else {
            None
        }
    }


    /// # Safety 
    /// safe iff there are no existing references to data or next_data (i.e. this should be called immediately after
    /// synchronize
    pub unsafe fn raw_reclaim(lock: &mut InnerGuard<'_, (T, Option<T>)>) -> Option<T>{
        let (data, new_data) = lock.as_mut();
            new_data
                .take()
                .map(|new_data| std::mem::replace(data, new_data))
    }


    fn _current_mut(lock:&'a mut InnerGuard<'_, (T, Option<T>)>) -> &'a mut T{
        &mut lock.as_mut().0
    }

    fn next_mut(lock:&'a mut InnerGuard<'_, (T, Option<T>)>) -> &'a mut Option<T>{
        &mut lock.as_mut().1
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
