//this is so fucked up
//
//
use std::sync::Arc;
struct EvilRcu<T:Sync>(Vec<Arc<T>>);

struct RcuReadGuard<T>(Arc<T>);



impl<T:Sync> EvilRcu<T>{
    fn new(data:T) -> Self{

        let mut inner = Vec::new();
        inner.push(Arc::new(data));
        Self(
            inner
        )
    }


    fn end(&self) -> &Arc<T>{
        &self.0.get(self.0.len()).expect("Somehow Rcu ended up empty")
    }

    fn read(&self) -> RcuReadGuard<T>{
        RcuReadGuard(self.end().clone())
    }

    fn update(&mut self, data:T){
        self.0.push(Arc::new(data))
    }

    fn clean(&mut self){
        // make sure we don't accidentally delete everything
        let safety_guard = self.end().clone();
        self.0.retain(|x| Arc::strong_count(x) >=1);   
        drop(safety_guard);
    }

    fn synchronize(&mut self){
        while self.0.len() > 1{
            self.clean();
        }
    }
}

