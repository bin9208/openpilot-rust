use openpilot_process_supervision::{CapturedCommand, Error};
use signal_hook::{
    consts::{SIGCHLD, SIGINT},
    iterator::{Handle, Signals},
};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
};

#[derive(Default)]
struct State {
    active: bool,
    interrupts: Vec<bool>,
}
pub struct Children {
    state: Arc<(Mutex<State>, Condvar)>,
    exit: Arc<AtomicBool>,
    handle: Handle,
    thread: Option<JoinHandle<()>>,
}
impl Children {
    pub fn new() -> io::Result<Self> {
        let mut signals = Signals::new([SIGINT, SIGCHLD])?;
        let handle = signals.handle();
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let exit = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_exit = exit.clone();
        let thread = std::thread::Builder::new()
            .name("panda-signals".into())
            .spawn(move || {
                for signal in &mut signals {
                    let (mutex, wake) = &*worker_state;
                    let mut state = match mutex.lock() {
                        Ok(state) => state,
                        Err(_) => break,
                    };
                    if signal == SIGINT {
                        worker_exit.store(true, Ordering::Release);
                        let forward = state.active;
                        state.interrupts.push(forward);
                    }
                    wake.notify_all();
                }
            })?;
        Ok(Self {
            state,
            exit,
            handle,
            thread: Some(thread),
        })
    }
    pub fn requested(&self) -> bool {
        self.exit.load(Ordering::Acquire)
    }
    pub fn take_interrupts(&self) -> io::Result<usize> {
        let mut state = self
            .state
            .0
            .lock()
            .map_err(|_| io::Error::other("Panda child lock poisoned"))?;
        Ok(std::mem::take(&mut state.interrupts).len())
    }
    pub fn run(
        &self,
        command: &CapturedCommand,
        mut interrupted: impl FnMut() -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut child =
            command.spawn_inherited_with_env(&[("MANAGER_DAEMON".into(), "pandad".into())])?;
        let (mutex, wake) = &*self.state;
        let mut state = mutex
            .lock()
            .map_err(|_| io::Error::other("Panda child lock poisoned"))?;
        state.active = true;
        loop {
            for forward in std::mem::take(&mut state.interrupts) {
                if let Err(error) = interrupted() {
                    state.active = false;
                    return Err(error);
                }
                if forward {
                    match child.process.try_wait() {
                        Ok(None) => {
                            let pid = i32::try_from(child.process.id())
                                .ok()
                                .and_then(rustix::process::Pid::from_raw)
                                .ok_or_else(|| io::Error::other("invalid Panda child PID"))?;
                            match rustix::process::kill_process(pid, rustix::process::Signal::INT) {
                                Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                                Err(error) => {
                                    state.active = false;
                                    return Err(io::Error::from(error).into());
                                }
                            }
                        }
                        Ok(Some(_)) => (),
                        Err(error) => {
                            state.active = false;
                            return Err(error.into());
                        }
                    }
                }
            }
            match child.process.try_wait() {
                Ok(Some(_)) => {
                    state.active = false;
                    return Ok(());
                }
                Err(error) => {
                    state.active = false;
                    return Err(error.into());
                }
                Ok(None) => (),
            }
            state = wake
                .wait(state)
                .map_err(|_| io::Error::other("Panda child lock poisoned"))?;
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
