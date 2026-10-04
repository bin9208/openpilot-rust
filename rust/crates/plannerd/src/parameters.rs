use crate::Error;

pub trait Parameters {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error>;
    fn float(&mut self, key: &'static str) -> Result<f64, Error>;
}
