pub mod evil_rcu;

pub trait Rcu<'a, T: Sync + 'a> {
    type RcuReadGuard;
    type RcuWriteGuard;
    fn read(&'a self) -> Self::RcuReadGuard;

    fn write(&'a self) -> Self::RcuWriteGuard;

    fn mut_write(&'a mut self) -> Self::RcuWriteGuard;

    fn update_now(&self, data: T);

    fn mut_update(&mut self, data: T);

    fn synchronize(&self, guard: &mut Self::RcuWriteGuard);
}

mod async_ext {
    use super::*;
    use std::future::Future;
    pub trait AsyncRcu<'a, T: Sync + 'a>: Rcu<'a, T> {
        fn poll_write(&'a self) -> impl Future<Output = Option<Self::RcuWriteGuard>>;

        fn poll_update(&self, data: T) -> impl Future<Output = ()>;
    }
}
