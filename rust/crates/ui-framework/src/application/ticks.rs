use crate::Error;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone)]
pub struct Tick(Rc<RefCell<dyn FnMut()>>);
impl Tick {
    pub fn new(callback: impl FnMut() + 'static) -> Self {
        Self(Rc::new(RefCell::new(callback)))
    }
}
#[derive(Clone, Default)]
pub struct TickRegistry(Rc<RefCell<Vec<Tick>>>);
impl TickRegistry {
    pub fn add(&self, tick: Tick) {
        let mut ticks = self.0.borrow_mut();
        if !ticks.iter().any(|item| Rc::ptr_eq(&item.0, &tick.0)) {
            ticks.push(tick);
        }
    }
    pub fn remove(&self, tick: &Tick) {
        self.0
            .borrow_mut()
            .retain(|item| !Rc::ptr_eq(&item.0, &tick.0));
    }
    pub fn run(&self) -> Result<(), Error> {
        let mut index = 0usize;
        loop {
            let tick = self.0.borrow().get(index).cloned();
            let Some(tick) = tick else { break };
            (tick
                .0
                .try_borrow_mut()
                .map_err(|_| Error::Contract("navigation tick recursively borrowed"))?)(
            );
            index = index
                .checked_add(1)
                .ok_or(Error::Contract("tick index overflow"))?;
        }
        Ok(())
    }
}
