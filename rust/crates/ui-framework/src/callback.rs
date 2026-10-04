use std::{cell::RefCell, rc::Rc};
pub struct Callback<T>(Rc<RefCell<dyn FnMut(T)>>);
impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T> Callback<T> {
    pub fn new(callback: impl FnMut(T) + 'static) -> Self {
        Self(Rc::new(RefCell::new(callback)))
    }
    pub fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    pub fn call(&self, value: T) {
        (self.0.borrow_mut())(value);
    }
}
