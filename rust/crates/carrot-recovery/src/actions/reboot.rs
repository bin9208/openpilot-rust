use crate::{config::Config, Error};
use openpilot_process_supervision::CapturedChild;
use rustix::process::{Pid, Signal, WaitId, WaitIdOptions};
use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

struct Child {
    child: Option<CapturedChild>,
    leader: Pid,
}
impl Child {
    fn cleanup(&mut self) -> Result<(), Error> {
        if let Some(child) = &mut self.child {
            match rustix::process::kill_process_group(self.leader, Signal::KILL) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
            crate::terminal::session::cleanup(self.leader)?;
            child.process.wait()?;
            self.child = None;
        }
        Ok(())
    }
    fn run(mut self, stopped: &AtomicBool) -> Result<(), Error> {
        let observed = loop {
            match rustix::process::waitid(
                WaitId::Pid(self.leader),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(Some(_)) => break Ok(()),
                Ok(None) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => break Err(std::io::Error::from(error).into()),
            }
            if stopped.load(Ordering::SeqCst) {
                break Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        };
        self.cleanup()?;
        observed
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("[recovery] reboot child cleanup: {error}");
        }
    }
}
struct Worker {
    stopped: Arc<AtomicBool>,
    task: JoinHandle<Result<(), Error>>,
}
static WORKERS: OnceLock<Mutex<Vec<Worker>>> = OnceLock::new();
fn workers() -> &'static Mutex<Vec<Worker>> {
    WORKERS.get_or_init(|| Mutex::new(Vec::new()))
}
fn joined(worker: Worker) -> Result<(), Error> {
    worker
        .task
        .join()
        .map_err(|_| Error::Source("reboot child worker panicked".into()))?
}

pub(super) fn spawn(config: &Config, args: &[&str]) -> Result<(), Error> {
    let mut workers = workers()
        .lock()
        .map_err(|_| Error::Source("reboot worker registry poisoned".into()))?;
    while let Some(index) = workers.iter().position(|worker| worker.task.is_finished()) {
        joined(workers.swap_remove(index))?;
    }
    let child = super::process::request(config, args, &std::env::current_dir()?)
        .spawn_session_redirected(Stdio::null(), Stdio::null())?;
    let pid = i32::try_from(child.process.id())
        .map_err(|_| Error::Source("reboot PID out of range".into()))?;
    let leader = Pid::from_raw(pid).ok_or_else(|| Error::Source("invalid reboot PID".into()))?;
    let child = Child {
        child: Some(child),
        leader,
    };
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&stopped);
    let task = thread::Builder::new()
        .name("recovery-reboot".into())
        .spawn(move || child.run(&stop))?;
    workers.push(Worker { stopped, task });
    Ok(())
}

pub(super) fn shutdown() -> Result<(), Error> {
    let workers = {
        let mut workers = workers()
            .lock()
            .map_err(|_| Error::Source("reboot worker registry poisoned".into()))?;
        for worker in workers.iter() {
            worker.stopped.store(true, Ordering::SeqCst);
        }
        std::mem::take(&mut *workers)
    };
    let mut failed = None;
    for worker in workers {
        if let Err(error) = joined(worker) {
            eprintln!("[recovery] reboot worker: {error}");
            if failed.is_none() {
                failed = Some(error);
            }
        }
    }
    match failed {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
