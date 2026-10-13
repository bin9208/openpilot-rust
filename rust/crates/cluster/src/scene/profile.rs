use std::time::Instant;

pub type ProfileAdd<'a> = Option<&'a mut dyn FnMut(&str, f64)>;

pub(super) fn record(profile: &mut ProfileAdd<'_>, name: &str, started: Instant) {
    if let Some(add) = profile {
        add(name, started.elapsed().as_secs_f64() * 1000.0);
    }
}

pub(super) struct ModelProfile<'a, 'b> {
    pub callback: &'a mut ProfileAdd<'b>,
    pub prefix: &'static str,
}
impl ModelProfile<'_, '_> {
    pub fn record(&mut self, suffix: &str, started: Instant) {
        if let Some(add) = self.callback {
            add(
                &format!("{}.{suffix}", self.prefix),
                started.elapsed().as_secs_f64() * 1000.0,
            );
        }
    }
    pub fn count(&mut self, suffix: &str) {
        if let Some(add) = self.callback {
            add(&format!("{}.{suffix}", self.prefix), 0.0);
        }
    }
}
