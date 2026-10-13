use std::time::Instant;

pub(super) struct Samples {
    pub enabled: bool,
    pub values: Vec<(&'static str, f64)>,
}

impl Samples {
    pub fn from_environment() -> Self {
        Self {
            enabled: std::env::var("CLUSTER_PROFILE_USB").is_ok_and(|value| value == "1"),
            values: Vec::new(),
        }
    }

    pub fn start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }

    pub fn add(&mut self, name: &'static str, started: Option<Instant>) {
        if self.enabled {
            if let Some(started) = started {
                self.values
                    .push((name, started.elapsed().as_secs_f64() * 1000.0));
            }
        }
    }
}
