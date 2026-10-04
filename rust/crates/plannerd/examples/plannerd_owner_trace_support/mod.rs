pub mod decode;
pub mod snapshot;
use openpilot_plannerd::{native_parameters, parameters::Parameters, Error};
use std::collections::BTreeMap;

pub struct Store {
    pub values: BTreeMap<String, String>,
    pub operations: Vec<[String; 2]>,
}

impl Parameters for Store {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        self.operations.push(["get_int".into(), key.into()]);
        native_parameters::integer(self.values.get(key).map_or(&[], |value| value.as_bytes()))
    }
    fn float(&mut self, key: &'static str) -> Result<f64, Error> {
        self.operations.push(["get_float".into(), key.into()]);
        native_parameters::float(self.values.get(key).map_or(&[], |value| value.as_bytes()))
    }
}
