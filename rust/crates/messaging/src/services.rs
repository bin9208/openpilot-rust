use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Service {
    pub name: &'static str,
    pub should_log: bool,
    pub frequency: f64,
    pub decimation: Option<u32>,
    pub queue_size: usize,
    pub frequency_range: Option<(f64, f64)>,
}

include!("services_generated.rs");

pub fn lookup(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.name == name)
}
