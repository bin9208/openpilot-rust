use openpilot_cereal::log_capnp::event;
use openpilot_manager_catalog::{catalog, ImportConfig};
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsFd,
        unix::{fs::symlink, net::UnixListener},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct Probe {
    pub params: openpilot_params::Params,
    output: PathBuf,
    command: Command,
    child: Option<Child>,
    lock: File,
    contender: File,
    listener: UnixListener,
    _namespace: tempfile::TempDir,
    prefix: String,
    stats_socket_owned: bool,
}
impl Probe {
    pub fn new(output: &Path, root: &Path, binaries: &Path, mode: &str) -> Result<Self> {
        let display = std::env::var_os("DISPLAY");
        let raylib = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY");
        for (key, _) in std::env::vars_os() {
            std::env::remove_var(key);
        }
        std::fs::create_dir_all(output)?;
        if mode == "normal" && Path::new("/tmp/stats").exists() {
            return Err("stats socket already belongs to another runtime".into());
        }
        let namespace = tempfile::Builder::new()
            .prefix("msgq_rust-probe-installed-")
            .tempdir_in("/dev/shm")?;
        let prefix = namespace
            .path()
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(|value| value.strip_prefix("msgq_"))
            .ok_or("namespace missing")?
            .to_owned();
        std::env::set_var("OPENPILOT_PREFIX", &prefix);
        std::env::set_var("PARAMS_ROOT", output.join("params"));
        let params = openpilot_params::Params::for_runtime()?;
        let bindir = output.join("bin");
        let tools = output.join("tools");
        std::fs::create_dir_all(&bindir)?;
        std::fs::create_dir_all(&tools)?;
        symlink("/usr/bin/git", tools.join("git"))?;
        for name in [
            "openpilot-manager",
            "openpilot-process-child",
            "openpilot-bootlog",
            "openpilot-logmessaged",
            "statsd-rs",
        ] {
            if (mode == "missing-daemon" && name == "statsd-rs")
                || (mode.starts_with("missing-bootlog") && name == "openpilot-bootlog")
            {
                continue;
            }
            std::fs::copy(binaries.join(name), bindir.join(name))?;
        }
        if mode == "missing-bootlog-ui" {
            std::fs::copy(
                binaries.join("openpilot-text-window"),
                bindir.join("openpilot-text-window"),
            )?;
        }
        for name in ["home", "tmp", "logs"] {
            std::fs::create_dir_all(output.join(name))?;
        }
        let lock_path = output.join("repo.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&lock_path)?;
        lock.lock()?;
        let contender = OpenOptions::new().read(true).write(true).open(&lock_path)?;
        assert!(contender.try_lock().is_err());
        let socket = namespace.path().join("lock.sock");
        let listener = UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let block = catalog(ImportConfig::from_environment(false))
            .into_iter()
            .filter(|entry| !matches!(entry.name, "statsd" | "logmessaged"))
            .map(|entry| entry.name)
            .collect::<Vec<_>>()
            .join(",");
        let log = File::create(output.join("manager.log"))?;
        let mut command = Command::new(bindir.join("openpilot-manager"));
        command
            .args(["--root"])
            .arg(root)
            .arg("--bin-dir")
            .arg(&bindir)
            .env_clear()
            .env("PATH", &tools)
            .env("HOME", output.join("home"))
            .env("TMPDIR", output.join("tmp"))
            .env("OPENPILOT_PREFIX", &prefix)
            .env("PARAMS_ROOT", output.join("params"))
            .env("LOG_ROOT", output.join("logs"))
            .env("LOGPRINT", "info")
            .env("BLOCK", block)
            .env("NOBOARD", "1")
            .env("CARROT_REPO_LOCK_PATH", lock_path)
            .env("CARROT_BOOT_LOCK_SOCKET", socket)
            .stdout(log.try_clone()?)
            .stderr(log);
        if mode.starts_with("missing-bootlog") {
            command.env("PREPAREONLY", "1");
        }
        if mode == "missing-bootlog-ui" {
            command
                .env("DISPLAY", display.ok_or("UI display missing")?)
                .env(
                    "STARTUP_UI_RAYLIB_LIBRARY",
                    raylib.ok_or("UI provider missing")?,
                );
        }
        Ok(Self {
            params,
            output: output.into(),
            command,
            child: None,
            lock,
            contender,
            listener,
            _namespace: namespace,
            prefix,
            stats_socket_owned: mode == "normal",
        })
    }
    pub fn model_assets_override(&mut self, path: &Path) {
        self.command.env("USBGPU_ASSETS_ROOT", path);
    }
    pub fn enable_route_logger(&mut self, binaries: &Path) -> Result<()> {
        std::fs::copy(
            binaries.join("openpilot-loggerd"),
            self.output.join("bin/openpilot-loggerd"),
        )?;
        let block = catalog(ImportConfig::from_environment(false))
            .into_iter()
            .filter(|entry| !matches!(entry.name, "statsd" | "logmessaged" | "loggerd"))
            .map(|entry| entry.name)
            .collect::<Vec<_>>()
            .join(",");
        self.command.env("BLOCK", block);
        Ok(())
    }
    pub fn stdout_pressure(&mut self, full: bool) -> Result<Option<std::io::PipeReader>> {
        let (reader, writer) = std::io::pipe()?;
        if full {
            rustix::fs::fcntl_setfl(&writer, rustix::fs::OFlags::NONBLOCK)?;
            loop {
                match rustix::io::write(&writer, &[b'X'; 4096]) {
                    Ok(_) => {}
                    Err(rustix::io::Errno::AGAIN) => break,
                    Err(error) => return Err(error.into()),
                }
            }
            rustix::fs::fcntl_setfl(&writer, rustix::fs::OFlags::empty())?;
        }
        self.command.stdout(writer.try_clone()?).stderr(writer);
        Ok(if full { Some(reader) } else { None })
    }
    pub fn handoff(&mut self, executable: &Path) -> Result<()> {
        let mut command = Command::new(executable);
        command
            .arg(self.command.get_program())
            .args(self.command.get_args())
            .env_clear()
            .envs(
                self.command
                    .get_envs()
                    .filter_map(|(key, value)| value.map(|value| (key, value))),
            );
        if let Some(directory) = self.command.get_current_dir() {
            command.current_dir(directory);
        }
        let log = File::create(self.output.join("manager.log"))?;
        command.stdout(log.try_clone()?).stderr(log);
        self.command = command;
        Ok(())
    }
    pub fn signal(&self, signal: &str) -> Result<()> {
        let child = self.child.as_ref().ok_or("manager missing")?;
        assert!(Command::new("/bin/kill")
            .args([signal, &child.id().to_string()])
            .status()?
            .success());
        Ok(())
    }
    pub fn launch(&mut self) -> Result<()> {
        self.child = Some(self.command.spawn()?);
        Ok(())
    }
    pub fn transfer_lock(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match self.listener.accept() {
                Ok((mut connection, _)) => {
                    openpilot_process_supervision::send_boot_lock(
                        &mut connection,
                        self.lock.as_fd(),
                    )?;
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err("manager boot-lock connection timeout".into());
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
    pub fn lock_released(&self) -> Result<bool> {
        match self.contender.try_lock() {
            Ok(()) => {
                self.contender.unlock()?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
    pub fn terminate(&self) -> Result<()> {
        self.signal("-TERM")
    }
    pub fn wait(&mut self) -> Result<ExitStatus> {
        let child = self.child.as_mut().ok_or("manager missing")?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait()? {
                self.child = None;
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err("manager did not exit".into());
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    pub fn log_files(&self) -> Result<Vec<PathBuf>> {
        let directory = self
            .output
            .join("home")
            .join(format!(".comma{}", self.prefix))
            .join("log");
        Ok(std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?)
    }
    pub fn write_summary(&self, summary: serde_json::Value) -> Result<()> {
        std::fs::write(
            self.output.join("summary.json"),
            serde_json::to_vec_pretty(&summary)?,
        )?;
        Ok(())
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            match Command::new("/bin/kill")
                .args(["-TERM", &child.id().to_string()])
                .status()
            {
                Ok(status) if status.success() => {}
                Ok(status) => eprintln!("probe manager terminate: {status}"),
                Err(error) => eprintln!("probe manager terminate: {error}"),
            }
            std::thread::sleep(Duration::from_secs(2));
            if matches!(child.try_wait(), Ok(None)) {
                if let Err(error) = child.kill() {
                    eprintln!("probe manager kill: {error}");
                }
            }
            if let Err(error) = child.wait() {
                eprintln!("probe manager wait: {error}");
            }
        }
        if self.stats_socket_owned && Path::new("/tmp/stats").exists() {
            if let Err(error) = std::fs::remove_file("/tmp/stats") {
                eprintln!("probe stats IPC cleanup: {error}");
            }
        }
        let ipc = PathBuf::from(format!("/tmp/logmessage{}", self.prefix));
        if ipc.exists() {
            if let Err(error) = std::fs::remove_file(ipc) {
                eprintln!("probe logging IPC cleanup: {error}");
            }
        }
    }
}
pub fn packet(topic: &str, started: bool) -> Result<Vec<u8>> {
    let mut message = capnp::message::Builder::new_default();
    let mut event = message.init_root::<event::Builder>();
    // Invalid synthetic device packets exercise transitions without host watchdog writes.
    event.set_valid(false);
    match topic {
        "deviceState" => event.init_device_state().set_started(started),
        "carParams" => event.init_car_params().set_not_car(false),
        "pandaStates" => {
            event.init_panda_states(0);
        }
        _ => return Err("unknown input topic".into()),
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
