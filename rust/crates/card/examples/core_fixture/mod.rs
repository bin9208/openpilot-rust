mod io;
mod vehicle;
pub use io::Io;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use vehicle::{Driver, Tail};

#[derive(Deserialize)]
pub struct Case {
    pub params: Vec<u8>,
    pub replay: bool,
    pub has_controller: bool,
    pub steps: Vec<Step>,
}
#[derive(Clone, Deserialize)]
pub struct Step {
    pub now: u64,
    pub can: Vec<Vec<u8>>,
    pub messages: Vec<Vec<u8>>,
    pub state: Vec<u8>,
    pub accel: f32,
    pub settings: BTreeMap<String, String>,
    pub remaining: f64,
}
#[derive(Serialize)]
pub struct Publication {
    pub topic: String,
    pub wire: Vec<u8>,
}
#[derive(Default, Serialize)]
pub struct Output {
    pub param_writes: Vec<(String, Vec<u8>)>,
    pub publications: Vec<Publication>,
    pub calls: Vec<serde_json::Value>,
    pub warnings: Vec<String>,
    pub diagnostics: Vec<(&'static str, f64)>,
    pub initialized: bool,
    pub timeouts: u64,
    pub settings: BTreeMap<String, bool>,
    pub error: Option<String>,
}
