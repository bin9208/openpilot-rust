use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
#[derive(Default)]
struct Scope {
    term_default: Option<Arc<AtomicBool>>,
    int_default: Option<Arc<AtomicBool>>,
    active: usize,
    managed: usize,
}
static SCOPE: Mutex<Scope> = Mutex::new(Scope {
    term_default: None,
    int_default: None,
    active: 0,
    managed: 0,
});
impl Scope {
    fn prepare(&mut self) -> io::Result<()> {
        if self.term_default.is_none() {
            let term = Arc::new(AtomicBool::new(true));
            let interrupt = Arc::new(AtomicBool::new(true));
            signal_hook::flag::register_conditional_default(
                signal_hook::consts::SIGTERM,
                Arc::clone(&term),
            )?;
            signal_hook::flag::register_conditional_default(
                signal_hook::consts::SIGINT,
                Arc::clone(&interrupt),
            )?;
            self.term_default = Some(term);
            self.int_default = Some(interrupt);
        }
        Ok(())
    }
    fn update(&self) {
        if let Some(default) = &self.term_default {
            default.store(self.active == 0, Ordering::SeqCst);
        }
        if let Some(default) = &self.int_default {
            default.store(self.active == 0 && self.managed == 0, Ordering::SeqCst);
        }
    }
}
pub(super) struct ManagedInterrupt {
    requested: Arc<AtomicBool>,
    registration: signal_hook::SigId,
}
impl ManagedInterrupt {
    pub fn install() -> io::Result<Self> {
        let mut scope = SCOPE
            .lock()
            .map_err(|_| io::Error::other("cluster signal scope poisoned"))?;
        scope.prepare()?;
        let requested = Arc::new(AtomicBool::new(false));
        let registration =
            signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&requested))?;
        scope.managed += 1;
        scope.update();
        Ok(Self {
            requested,
            registration,
        })
    }
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.requested)
    }
}
impl Drop for ManagedInterrupt {
    fn drop(&mut self) {
        match SCOPE.lock() {
            Ok(mut scope) => {
                scope.managed = scope.managed.saturating_sub(1);
                scope.update();
            }
            Err(error) => eprintln!("cluster managed signal cleanup: {error}"),
        }
        signal_hook::low_level::unregister(self.registration);
    }
}

pub(super) struct Stop {
    requested: Arc<AtomicBool>,
    registration: signal_hook::SigId,
}
impl Stop {
    pub fn install() -> io::Result<Self> {
        let mut scope = SCOPE
            .lock()
            .map_err(|_| io::Error::other("cluster signal scope poisoned"))?;
        scope.prepare()?;
        let requested = Arc::new(AtomicBool::new(false));
        let registration =
            signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&requested))?;
        scope.active += 1;
        scope.update();
        Ok(Self {
            requested,
            registration,
        })
    }
    pub fn requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }
}
impl Drop for Stop {
    fn drop(&mut self) {
        match SCOPE.lock() {
            Ok(mut scope) => {
                scope.active = scope.active.saturating_sub(1);
                scope.update();
            }
            Err(error) => eprintln!("cluster signal cleanup: {error}"),
        }
        signal_hook::low_level::unregister(self.registration);
    }
}

#[cfg(test)]
mod tests {
    use super::Stop;
    use std::{os::unix::process::ExitStatusExt, process::Command};

    #[test]
    fn signal_child() {
        if std::env::var_os("CLUSTER_STOP_CHILD").is_none() {
            return;
        }
        for index in 0..2 {
            let stop = Stop::install().unwrap();
            let interrupt = openpilot_managed_entry::Sigint::install().unwrap();
            assert!(!stop.requested());
            signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
            assert!(stop.requested());
            signal_hook::low_level::raise(signal_hook::consts::SIGINT).unwrap();
            assert!(interrupt.requested());
            drop(interrupt);
            drop(stop);
            println!("closed:{index}");
        }
        let signal = std::env::var("CLUSTER_STOP_FINAL_SIGNAL")
            .unwrap()
            .parse()
            .unwrap();
        signal_hook::low_level::raise(signal).unwrap();
        println!("incorrectly survived signal after close");
    }

    #[test]
    fn signal_defaults_return_after_each_owned_run() {
        let mut records = Vec::new();
        for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
            let result = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "runtime::stop::tests::signal_child",
                    "--nocapture",
                ])
                .env("CLUSTER_STOP_CHILD", "1")
                .env("CLUSTER_STOP_FINAL_SIGNAL", signal.to_string())
                .output()
                .unwrap();
            records.push(serde_json::json!({"requested_signal":signal,
                "signal":result.status.signal(),"exit":result.status.code(),
                "stdout":String::from_utf8_lossy(&result.stdout),
                "stderr":String::from_utf8_lossy(&result.stderr)}));
        }
        if let Some(path) = std::env::var_os("CLUSTER_STOP_EVIDENCE") {
            std::fs::write(path, serde_json::to_vec_pretty(&records).unwrap()).unwrap();
        }
        for record in records {
            assert_eq!(record["signal"], record["requested_signal"]);
            assert!(record["stdout"].as_str().unwrap().contains("closed:1"));
        }
    }
}
