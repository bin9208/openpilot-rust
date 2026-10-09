use crate::Value;
use openpilot_params::Params;
use std::collections::HashMap;

pub(super) struct Settings {
    params: Option<Params>,
    cache: HashMap<String, (f64, Value)>,
}
impl Settings {
    pub fn new(params: Option<Params>) -> Self {
        Self {
            params,
            cache: HashMap::new(),
        }
    }
    pub fn boolean(&mut self, name: &str, mono: f64) -> bool {
        let key = format!("bool:{name}");
        if let Some((at, value)) = self.cache.get(&key) {
            if mono - at < 0.5 {
                return value.truth();
            }
        }
        let value = self
            .params
            .as_ref()
            .and_then(|params| params.get(name).ok().flatten())
            .is_some_and(|bytes| bytes == b"1");
        self.cache.insert(key, (mono, Value::Bool(value)));
        value
    }
    pub fn integer(&mut self, name: &str, mono: f64) -> i32 {
        let key = format!("int:{name}");
        if let Some((at, Value::Integer(value))) = self.cache.get(&key) {
            if mono - at < 0.5 {
                return num_traits::ToPrimitive::to_i32(value).unwrap_or(0);
            }
        }
        let bytes = self
            .params
            .as_ref()
            .and_then(|params| params.get(name).ok().flatten())
            .unwrap_or_default();
        let value = match openpilot_beepd::integer(&bytes) {
            Ok(value) => value,
            Err(error) => crate::param_native::fatal(name, &error),
        };
        self.cache.insert(key, (mono, Value::integer(value)));
        value
    }
}
