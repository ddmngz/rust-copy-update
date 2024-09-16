use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::sync::Arc;
use std::cell::UnsafeCell;


struct Reclaimer<T>{
    data: UnsafeCell<T>,
    readers: AtomicU32,
    writing: AtomicBool,
    new_data: UnsafeCell<Option<T>>,
}

struct RcuReadGuard<'a, T>{
    data: *const T,
    readers: &'a AtomicU32,
}

pub struct Rcu<T>{
    reclaimer:Arc<Reclaimer<T>>,
    write:Mutex<()>,
}

impl<T> Reclaimer<T>{
    fn new(data:T) -> Self{
        Self{
            data:data.into(),
            readers:0.into(),
            writing: false.into(),
            new_data:None.into(),
        }
    }

    fn read(&self) -> RcuReadGuard<'_, T>{
        self.readers.fetch_add(1,Ordering::Acquire);
        let data: *const T = self.data.get();
        let readers = &self.readers;
        RcuReadGuard{
            data,
            readers,
        }
    }


    fn reclaim(&self){
        let data = unsafe{self.data.get().as_mut()}.unwrap();
        let new_data = unsafe{self.new_data.get().as_mut()}.unwrap().take().expect("no new data");
        let _ = std::mem::replace(data, new_data);
    }

    fn synchronized(&self) -> bool{
        self.readers.load(Ordering::Acquire) == 0
    }

    fn update(&self, data:T){
        let new_data = self.new_data.get();
        let new_data = unsafe{new_data.as_mut().unwrap()};
        new_data.replace(data).expect("error! updated before reclaiming");
    }
}

impl<T> Rcu<T>{

    fn new(data:T) -> Self{
        let reclaimer = Arc::new(Reclaimer::new(data));
        Self{
            reclaimer,
            write:Mutex::new(()),
        }
    }

    fn synchronize(&self) {
        while !self.reclaimer.synchronized(){}
        self.reclaimer.reclaim();
    }
    
    fn read(&self) -> RcuReadGuard<'_,T>{
        self.reclaimer.read()
    }

    fn update(&self, data:T){
        self.reclaimer.update(data)
    }
}

impl<T> AsRef<T> for RcuReadGuard<'_,T>{
    fn as_ref(&self) -> &T{
        unsafe{self.data.as_ref().unwrap()}
    }
}

impl<T> Drop for RcuReadGuard<'_,T>{
    fn drop(&mut self){
        self.readers.fetch_sub(1,Ordering::AcqRel);
    }
}
