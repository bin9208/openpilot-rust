use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Cooperative signal observation at the one-second manager poll boundary.
/// Interrupts run cleanup but skip hardware exit actions, matching SystemExit.
pub struct Signals {
    term: Arc<AtomicBool>,
    registration: signal_hook::SigId,
    interrupt: openpilot_managed_entry::Sigint,
}
impl Signals {
    pub fn install() -> std::io::Result<Self> {
        let interrupt = openpilot_managed_entry::Sigint::install()?;
        let term = Arc::new(AtomicBool::new(false));
        let registration = signal_hook::flag::register(signal_hook::consts::SIGTERM, term.clone())?;
        Ok(Self {
            term,
            registration,
            interrupt,
        })
    }
    pub fn requested(&self) -> bool {
        self.term.load(Ordering::Relaxed) || self.interrupt.requested()
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        signal_hook::low_level::unregister(self.registration);
    }
}
