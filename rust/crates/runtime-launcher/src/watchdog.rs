use crate::Error;
use rustix::process::{kill_process, test_kill_process, Pid, Signal};
use std::{
    env,
    fs::{self, File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Owner {
    _lock: File,
    pid_file: PathBuf,
    pid: String,
}

impl Owner {
    fn acquire(lock_file: &Path, pid_file: PathBuf) -> Result<Option<Self>, Error> {
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_file)?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                println!(
                    "[carrot_web] another watchdog already holds {}; exiting",
                    lock_file.display()
                );
                return Ok(None);
            }
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        if let Ok(previous) = fs::read_to_string(&pid_file) {
            if let Some(pid) = previous.trim().parse::<i32>().ok().and_then(Pid::from_raw) {
                if pid != rustix::process::getpid() && test_kill_process(pid).is_ok() {
                    println!("[carrot_web] watchdog pid {pid} is already running; exiting");
                    return Ok(None);
                }
            }
        }
        let pid = format!("{}\n", std::process::id());
        fs::write(&pid_file, &pid)?;
        Ok(Some(Self {
            _lock: lock,
            pid_file,
            pid,
        }))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        if fs::read_to_string(&self.pid_file).is_ok_and(|value| value == self.pid) {
            let _ = fs::remove_file(&self.pid_file);
        }
    }
}

pub(crate) struct Server(pub(crate) Child);

impl Server {
    pub(crate) fn finish(&mut self, stopped: &AtomicBool) -> Result<ExitStatus, Error> {
        while !stopped.load(Ordering::Relaxed) {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(25));
        }
        if let Some(pid) = i32::try_from(self.0.id()).ok().and_then(Pid::from_raw) {
            let _ = kill_process(pid, Signal::TERM);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Some(status) = self.0.try_wait()? {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(25));
        }
        self.0.kill()?;
        Ok(self.0.wait()?)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn delay(duration: Duration, stopped: &AtomicBool) {
    let started = Instant::now();
    while !stopped.load(Ordering::Relaxed) && started.elapsed() < duration {
        thread::sleep((duration - started.elapsed().min(duration)).min(Duration::from_millis(25)));
    }
}

fn path(variable: &str, fallback: &str) -> PathBuf {
    env::var_os(variable).map_or_else(|| fallback.into(), PathBuf::from)
}

pub fn main() -> Result<(), Error> {
    let mut args = env::args_os().skip(1);
    let repository = args
        .next()
        .map_or_else(|| PathBuf::from("/data/openpilot"), PathBuf::from);
    if repository.as_os_str() == "--help" || repository.as_os_str() == "-h" {
        println!("usage: openpilot-carrot-web-watchdog [REPOSITORY]");
        return Ok(());
    }
    if args.next().is_some() {
        return Err(Error::Configuration(
            "expected at most one repository path".into(),
        ));
    }
    let repository = if repository.is_absolute() {
        repository
    } else {
        env::current_dir()?.join(repository)
    };
    let port = env::var("CARROT_WEB_PORT").unwrap_or_else(|_| "7000".into());
    port.parse::<u16>()
        .map_err(|_| Error::Configuration("invalid CARROT_WEB_PORT".into()))?;
    let interval = env::var("CARROT_WEB_RESTART_DELAY")
        .unwrap_or_else(|_| "2".into())
        .parse::<f64>()
        .map_err(|_| Error::Configuration("invalid CARROT_WEB_RESTART_DELAY".into()))?;
    let interval = Duration::try_from_secs_f64(interval)
        .map_err(|_| Error::Configuration("invalid CARROT_WEB_RESTART_DELAY".into()))?;
    let stopped = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(&stopped))?;
    }
    let Some(_owner) = Owner::acquire(
        &path("CARROT_WEB_LOCK_FILE", "/tmp/carrot_web_watchdog.lock"),
        path("CARROT_WEB_PID_FILE", "/tmp/carrot_web_watchdog.pid"),
    )?
    else {
        return Ok(());
    };
    while !stopped.load(Ordering::Relaxed) {
        match fs::canonicalize(&repository) {
            Ok(current) => {
                println!("[carrot_web] starting external carrot_server on port {port}");
                let child = Command::new(current.join("rust/bin/openpilot-carrot-server"))
                    .args(["--host", "0.0.0.0", "--port", &port])
                    .current_dir(&current)
                    .env("OPENPILOT_ROOT", &current)
                    .env("BASEDIR", &current)
                    .env("CARROT_WEB_EXTERNAL", "1")
                    .env_remove("CARROT_BOOT_LOCK_FD")
                    .spawn();
                match child {
                    Ok(child) => {
                        let status = Server(child).finish(&stopped)?;
                        println!(
                            "[carrot_web] carrot_server exited {status}; restarting in {}s",
                            interval.as_secs_f64()
                        );
                    }
                    Err(error) => eprintln!("[carrot_web] server launch failed: {error}"),
                }
            }
            Err(error) => eprintln!(
                "[carrot_web] checkout unavailable at {}: {error}; retrying in {}s",
                repository.display(),
                interval.as_secs_f64()
            ),
        }
        delay(interval, &stopped);
    }
    Ok(())
}
