use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;
use std::ptr::NonNull;

pub struct RcuReadGuard<'a, T> {
    data: NonNull<T>,
    readers: &'a AtomicU32,
}

impl<T> AsRef<T> for RcuReadGuard<'_, T> {
    fn as_ref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T> Drop for RcuReadGuard<'_, T> {
    fn drop(&mut self) {
        self.readers.fetch_sub(1, Ordering::AcqRel);
    }
}

pub struct NeedsReclaim();

pub struct Rcu<T: Sync> {
    data: InnerLock<T>,
    readers: AtomicU32,
    writing_lock: AtomicBool,
    new_data: InnerLock<Option<T>>,
}

#[derive(PartialEq)]
enum RcuState {
    OneRep,
    TwoRep,
    Synchronized,
}

struct InnerLock<T:Sync>{
    data:UnsafeCell<T>,
    inner_lock:AtomicBool,
}

impl<T:Sync> From<T> for InnerLock<T>{
    fn from(data:T) -> Self{
        Self{
            data:UnsafeCell::new(data),
            inner_lock:false.into(),
        }
    }
}


impl<T:Sync> InnerLock<T>{
    fn lock(&self) -> InnerGuard<T>{
        self.spin();
        let data = unsafe{self.data.get().as_ref()}.unwrap().into();
        let lock = (&self.inner_lock).into();
        InnerGuard{
            data,
            lock,
        }
    }



    fn spin(&self){
        while self.inner_lock.compare_exchange(false,true,Ordering::Acquire, Ordering::Relaxed).is_err() {}
    }
}

impl<T:Sync> AsRef<T> for InnerLock<T>{
    fn as_ref(&self) -> &T{
        unsafe{self.data.get().as_ref()}.unwrap()
    }
}

struct InnerGuard<T:Sync>{
    data:NonNull<T>,
    lock:NonNull<AtomicBool>,
}

impl<T:Sync> Drop for InnerGuard<T>{
    fn drop(&mut self){
        unsafe{self.lock.as_ref()}.store(false,Ordering::Release)
    }
}
impl<T:Sync> AsMut<T> for InnerGuard<T>{
    fn as_mut(&mut self) -> &mut T{
        unsafe{self.data.as_mut()}
    }
}

impl <T:Sync> InnerGuard<T>{
    fn replace(&mut self, new:T) -> T{
        std::mem::replace(unsafe{self.data.as_mut()}, new)
    }
}

impl <T:Sync> InnerGuard<Option<T>>{
    fn take(&mut self) -> Option<T>{
        unsafe{self.data.as_mut()}.take()
    }
}

impl<T: Sync> Rcu<T> {
    pub fn new(data: T) -> Self {
        let new_data:Option<T> = None;
        Self {
            data: data.into(),
            readers: 0.into(),
            writing_lock: false.into(),
            new_data: new_data.into(),
        }
    }

    fn get_state(&self) -> RcuState {
        if self.readers.load(Ordering::Acquire) == 0 {
            RcuState::Synchronized
        } else if self.new_data.as_ref().is_some() {
            RcuState::TwoRep
        } else {
            RcuState::OneRep
        }
    }

    pub fn read(&self) -> RcuReadGuard<'_, T> {
        self.readers.fetch_add(1, Ordering::Acquire);
        let data:NonNull<T> = (self.data.as_ref()).into();
        let readers = &self.readers;
        RcuReadGuard { data, readers }
    }
   
    pub fn synchronize(&self){
        while self.get_state() != RcuState::Synchronized{}
    }

    pub fn update(&self, data:T) -> Result<(),NeedsReclaim>{
        let mut new_data = self.new_data.lock();
        let _data_lock = self.data.lock();
        if self.get_state() == RcuState::TwoRep{
            return Err(NeedsReclaim{})
        }
        new_data.replace(Some(data));
        Ok(())
    }

    pub async fn update_later(&self, data:T) -> Result<(),NeedsReclaim>{
        self.async_reclaim().await;
        //unsafe{self.new_data.get().as_mut()}.unwrap().replace(data); 
        self.async_reclaim().await;
        Ok(())
    }

    pub async fn async_reclaim(&self){
        let (data, new_data) = self.await;
        if let Some(new_data) = new_data.take(){
            *(data.as_mut()) = new_data;
        }
    }

    pub fn reclaim(&self){
        let mut data = self.data.lock();
        let mut new_data = self.new_data.lock();
        if self.get_state() == RcuState::Synchronized{
            if let Some(new_data) = new_data.take(){
                *(data.as_mut()) = new_data;
            }
        }
    }



}
use std::future::Future;
use std::task::Poll;
use std::pin::Pin;
use std::task::Context;
impl<T:Sync> Future for Rcu<T>{
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>{
        if self.readers.load(Ordering::Acquire) > 0{
            Poll::Pending
        }else{
            Poll::Ready(())
        }
    }
}

impl<T:Sync> Future for &Rcu<T>{
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>{
        if self.readers.load(Ordering::Acquire) > 0{
            Poll::Pending
        }else{
            Poll::Ready(())
        }
    }
}
