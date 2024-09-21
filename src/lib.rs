pub mod evil_rcu;

//
// interface goal
// read is easy
// use write to either update synced or update unsynced

pub trait Rcu<'a, T: Sync + 'a> {
    type ReadGuard: AsRef<T>;
    type WriteGuard: RcuWriteGuard<'a, T>;

    fn read(&'a self) -> Self::ReadGuard;
    fn write(&'a self) -> Self::WriteGuard;
    fn synchronize(&self);
    fn update_now(&'a self, data: T) {
        let mut lock = self.write();
        lock.update_synced(data);
    }
}

// ensure we're synchronized before updating
trait RcuWriteGuard<'a, T: Sync + 'a> {
    fn update_unsynced(&mut self, data: T);
    fn synchronize(&self);
    fn update_synced(&mut self, data: T) {
        self.synchronize();
        self.update_unsynced(data);
    }
}

pub mod mut_rcu {
    use super::*;
    pub trait MutRcu<'a, T: Sync + 'a>: Rcu<'a, T> {
        fn mut_write(&'a mut self) -> Self::WriteGuard;
        fn mut_update_synced(&'a mut self, new: T) {
            let mut lock = self.mut_write();
            lock.update_synced(new);
        }

        fn mut_update_unsynced(&'a mut self, new: T) {
            let mut lock = self.mut_write();
            lock.update_unsynced(new);
        }
    }
}

pub mod async_ext {
    use super::*;
    use std::future::Future;
    pub trait AsyncRcu<'a, T: Sync + 'a>: Rcu<'a, T> {
        fn poll_write(&'a self) -> impl Future<Output = Self::WriteGuard>;

        fn poll_synchronize(&self) -> impl Future<Output = ()>;

        fn poll_update(&self, data: T) -> impl Future<Output = ()>;
    }
}
