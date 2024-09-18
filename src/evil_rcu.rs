//this is so fucked up wth

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

struct EvilRcu<T: Sync> {
    newest: EvilRcuInner<T>,
    vestiges: Mutex<Vec<Arc<T>>>,
}

impl<'a, T: Sync> EvilRcu<T> {
    fn new(data: T) -> Self {
        Self {
            newest: EvilRcuInner::new(Arc::new(data)),
            vestiges: Mutex::new(Vec::new()),
        }
    }

    fn read(&'a self) -> RcuReadGuard<'a, T> {
        let ref_count = self.newest.get_cloned();
        RcuReadGuard {
            ref_count,
            lock: &self,
        }
    }

    fn write(&'a self) -> RcuWriteGuard<'a, T> {
        RcuWriteGuard {
            data: self.vestiges.lock().unwrap(),
            lock: &self,
        }
    }

    fn try_write(&'a self) -> Option<RcuWriteGuard<'a, T>> {
        self.vestiges
            .try_lock()
            .ok()
            .map(|data| RcuWriteGuard { data, lock: &self })
    }

    fn update_now(&self, data: Arc<T>) {
        let old_news = self.newest.replace(data);
        let mut lock = self.write();
        lock.push(old_news);
        Self::clean(&mut lock);
        Self::synchronize(&mut lock);
    }

    fn update_lite(&self, data: Arc<T>) {
        let old_news = self.newest.replace(data);
        let mut lock = self.write();
        lock.push(old_news);
        Self::clean(&mut lock);
    }

    fn clean(guard: &mut RcuWriteGuard<T>) {
        guard.clean()
    }

    fn synchronize(guard: &mut RcuWriteGuard<T>) {
        guard.synchronize()
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
}

struct RcuReadGuard<'a, T: Sync> {
    ref_count: Arc<T>,
    lock: &'a EvilRcu<T>,
}

struct RcuWriteGuard<'a, T: Sync> {
    data: MutexGuard<'a, Vec<Arc<T>>>,
    lock: &'a EvilRcu<T>,
}

impl<T: Sync> RcuWriteGuard<'_, T> {
    fn push(&mut self, data: Arc<T>) {
        self.data.push(data)
    }

    fn clean(&mut self) {
        self.data.retain(|x| Arc::strong_count(x) >= 1);
    }

    fn synchronize(&mut self) {
        while self.data.len() > 1 {
            self.clean();
        }
    }

    fn update(&mut self, new: T) {
        self.lock.newest.replace(Arc::new(new));
    }
}
