use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub status: String,
    pub detail: String,
}
impl Snapshot {
    pub fn busy(&self) -> bool {
        matches!(
            self.status.as_str(),
            "updating" | "cleaning" | "rebooting" | "rebuild_rebooting"
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub state: Snapshot,
    pub pending_reboot: Option<String>,
}
struct Inner {
    state: Snapshot,
    pending_reboot: Option<String>,
}
#[derive(Clone)]
pub struct Update(Arc<Mutex<Inner>>);
impl Default for Update {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Inner {
            state: Snapshot {
                status: "idle".into(),
                detail: String::new(),
            },
            pending_reboot: None,
        })))
    }
}
/// Lock lifetime includes both preparation and the reboot request. Implementors
/// must release their lock on every return path. The native implementation stages
/// a verified source/package pair instead of mutating a live Rust checkout.
pub trait Backend {
    fn disable_prompt(&mut self);
    fn with_lock<T>(
        &mut self,
        run: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String>;
    fn pull(&mut self) -> Result<(String, bool), String>;
    fn request_rebuild(&mut self) -> Result<(), String>;
    fn reboot(&mut self) -> Result<(), String>;
}
impl Update {
    pub fn resume(pending_reboot: Option<String>) -> Self {
        let update = Self::default();
        update.0.lock().expect("recovery state lock").pending_reboot = pending_reboot;
        update
    }
    pub fn progress(&self) -> Progress {
        let inner = self.0.lock().expect("recovery state lock");
        Progress {
            state: inner.state.clone(),
            pending_reboot: inner.pending_reboot.clone(),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().expect("recovery state lock").state.clone()
    }
    /// Reservation is synchronous so repeated taps cannot start two workers.
    pub fn begin(&self, rebuild: bool) -> bool {
        let mut inner = self.0.lock().expect("recovery state lock");
        if inner.state.busy() {
            return false;
        }
        inner.state = Snapshot {
            status: if rebuild { "cleaning" } else { "updating" }.into(),
            detail: String::new(),
        };
        true
    }
    fn set(&self, status: &str, detail: impl Into<String>) {
        self.0.lock().expect("recovery state lock").state = Snapshot {
            status: status.into(),
            detail: detail.into(),
        };
    }
    fn failed(&self, error: String) {
        let start = error.char_indices().rev().nth(1999).map_or(0, |(i, _)| i);
        self.set("failed", &error[start..]);
    }
    pub fn run_update(&self, backend: &mut impl Backend, automatic: bool) {
        backend.disable_prompt();
        let result = backend.with_lock(|backend| {
            let (target, changed) = backend.pull()?;
            let retry = self
                .0
                .lock()
                .expect("recovery state lock")
                .pending_reboot
                .as_ref()
                == Some(&target);
            if automatic && !changed && !retry {
                self.set(
                    "waiting",
                    "No new commit. Waiting for a fix or network connection.",
                );
                return Ok(());
            }
            self.0.lock().expect("recovery state lock").pending_reboot = Some(target.clone());
            self.set("rebooting", target.chars().take(10).collect::<String>());
            backend.reboot()
        });
        if let Err(error) = result {
            self.failed(error);
        }
    }
    pub fn run_rebuild(&self, backend: &mut impl Backend) {
        let result = backend.with_lock(|backend| {
            backend.request_rebuild()?;
            self.set(
                "rebuild_rebooting",
                "Full rebuild requested. Rebuilding after reboot.",
            );
            backend.reboot()
        });
        if let Err(error) = result {
            self.failed(error);
        }
    }
}
#[cfg(test)]
mod tests;
