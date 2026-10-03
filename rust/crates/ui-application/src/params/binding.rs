use super::{
    store::{Mutation, Store},
    Read,
};
use crate::Error;
use std::rc::Rc;
#[derive(Clone)]
pub struct Binding {
    pub params: Rc<Store>,
    pub key: String,
    pub asynchronous: bool,
}
impl Binding {
    pub fn boolean(&self) -> Result<bool, Error> {
        self.params.boolean(&self.key)
    }
    pub fn integer(&self) -> Result<i32, Error> {
        Ok(super::typed::integer(self.params.as_ref(), &self.key, false)?.unwrap_or(0))
    }
    pub fn write_bool(&self, value: bool) -> Result<(), Error> {
        self.write(if value { b"1".to_vec() } else { b"0".to_vec() })
    }
    pub fn write_int(&self, value: usize) -> Result<(), Error> {
        self.write(value.to_string().into_bytes())
    }
    fn write(&self, value: Vec<u8>) -> Result<(), Error> {
        if self.asynchronous {
            self.params.put_nonblocking(Mutation {
                key: self.key.clone(),
                value,
            })
        } else {
            self.params.put(&self.key, &value)
        }
    }
}
