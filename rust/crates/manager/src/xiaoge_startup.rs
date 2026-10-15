//! Optional inference starts only after this onroad session has settled.
#[derive(Clone)]
pub struct Snapshot {
    pub times: [u64; 5],
    pub valid: bool,
    pub safety_ready: bool,
    pub engageable: bool,
    pub can_valid: bool,
    pub can_timeout: bool,
}

#[derive(Default)]
pub struct Gate {
    start_messages: Option<[u64; 5]>,
    ready_since: Option<f64>,
    last_check: Option<f64>,
    ready: bool,
}

impl Gate {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn ready(&self) -> bool {
        self.ready
    }
    pub fn needs_initial_messages(&self) -> bool {
        self.start_messages.is_none()
    }
    pub fn update(
        &mut self,
        started: bool,
        sample: &Snapshot,
        controls_ready: bool,
        now: f64,
    ) -> bool {
        if !started {
            *self = Self::default();
            return false;
        }
        let Some(initial) = self.start_messages else {
            self.start_messages = Some(sample.times);
            return false;
        };
        if self.ready {
            return true;
        }
        if self.last_check.is_some_and(|last| now - last > 1.) {
            self.ready_since = None;
        }
        self.last_check = Some(now);
        let fresh = sample.valid
            && sample
                .times
                .iter()
                .zip(initial)
                .all(|(now, before)| *now > before);
        if !(controls_ready
            && fresh
            && sample.safety_ready
            && sample.engageable
            && sample.can_valid
            && !sample.can_timeout)
        {
            self.ready_since = None;
            return false;
        }
        let since = *self.ready_since.get_or_insert(now);
        self.ready = now - since >= 0.5;
        self.ready
    }
}
