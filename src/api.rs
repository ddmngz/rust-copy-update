struct Rcu<T:Sync> {
    inner: RcuInner<T>,
    value: AtomicPtr<T>,
}


impl<'a, T:Sync> Rcu<T>{
    // never blocks, return a reader
    pub fn read(&'a self) -> RcuReadGuard<'a, T>;
    // synchronize and reclaim, and then update data to new
    pub fn update_now(&self, new:T);
    // same as update_now but returns futures
    pub async fn update_later(&'a self);

    // lock writes to self, now the user needs to synchronize and reclaim, but they can do it at
    // their leisure
    pub fn write(&'a self) -> RcuWriteGuard<'a, T>;
    // block until all readers are reading the same data
    fn synchronize(&self);
    // free old data
    fn reclaim(&self);
}

struct RcuReadGuard<'a, T:Sync>{
}

impl<'a, T:Sync> AsRef<T> for RcuReadGuard<'a, T>{
}

impl<'a, T:Sync> RcuWriteGuard<'a, T>{
    // synchronizes and reclaims, in case you want to do it at some other point i guess
    pub fn synchronize(&self)
}
