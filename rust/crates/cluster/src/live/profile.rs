//! Real elapsed samples; native car-frame projection includes frame-to-state conversion.
use super::Input;
use std::{cell::RefCell, time::Instant};

#[derive(Default)]
pub(super) struct Profile {
    enabled: bool,
    samples: RefCell<Vec<(String, f64)>>,
}
impl Profile {
    pub(super) fn start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }
    pub(super) fn add(&self, name: &str, start: Option<Instant>) {
        if let Some(start) = start {
            self.samples
                .borrow_mut()
                .push((name.to_owned(), start.elapsed().as_secs_f64() * 1000.0));
        }
    }
}
impl Input {
    pub fn set_profile_enabled(&mut self, enabled: bool) {
        self.profile.enabled = enabled;
    }
    /// Returns and clears samples from completed native work.
    #[must_use]
    pub fn profile_samples(&self) -> Vec<(String, f64)> {
        std::mem::take(&mut *self.profile.samples.borrow_mut())
    }
    pub fn clear_profile_samples(&self) {
        self.profile.samples.borrow_mut().clear();
    }
    /// Closes the owned navigation-media worker. A busy worker remains owned for retry.
    #[must_use]
    pub fn close(&mut self) -> bool {
        self.media
            .as_mut()
            .is_none_or(crate::media::MediaInput::close)
    }
}
