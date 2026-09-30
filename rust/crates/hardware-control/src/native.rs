use crate::{AmplifierAction, Command, CommandOutput, Error, Platform};
use openpilot_amplifier::{Amplifier, LinuxPlatform as AmplifierPlatform};
use openpilot_process_supervision::CapturedCommand;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    time::Duration,
};

pub trait Commands {
    fn run(&mut self, command: &Command) -> Result<CommandOutput, Error>;
}

pub struct ProcessCommands {
    pub launcher: PathBuf,
}

impl Commands for ProcessCommands {
    fn run(&mut self, command: &Command) -> Result<CommandOutput, Error> {
        let argv = match command {
            Command::Shell(text) => vec!["/bin/sh".into(), "-c".into(), text.clone()],
            Command::Call(argv) | Command::Output(argv) => argv.clone(),
        };
        let request = CapturedCommand {
            launcher: self.launcher.clone(),
            cwd: std::env::current_dir().map_err(|error| Error::io(error, "."))?,
            argv: argv.iter().map(Into::into).collect(),
        };
        match command {
            Command::Output(_) => {
                let child = request.spawn_stdout().map_err(launch_error)?;
                let output = child
                    .process
                    .wait_with_output()
                    .map_err(|error| Error::io(error, "child"))?;
                Ok(CommandOutput {
                    status: output
                        .status
                        .code()
                        .unwrap_or_else(|| -output.status.signal().unwrap_or(1)),
                    stdout: output.stdout,
                })
            }
            Command::Shell(_) | Command::Call(_) => {
                let mut child = match request.spawn_inherited() {
                    Ok(child) => child,
                    Err(_) if matches!(command, Command::Shell(_)) => {
                        return Ok(CommandOutput {
                            status: -1,
                            stdout: Vec::new(),
                        })
                    }
                    Err(error) => return Err(launch_error(error)),
                };
                let status = child
                    .process
                    .wait()
                    .map_err(|error| Error::io(error, "child"))?;
                Ok(CommandOutput {
                    status: status
                        .code()
                        .unwrap_or_else(|| -status.signal().unwrap_or(1)),
                    stdout: Vec::new(),
                })
            }
        }
    }
}

pub struct LinuxPlatform<C = ProcessCommands> {
    root: PathBuf,
    pub commands: C,
    amplifier: AmplifierPlatform,
}

impl<C: Commands> LinuxPlatform<C> {
    pub fn new(root: &Path, commands: C) -> Self {
        Self {
            root: root.into(),
            commands,
            amplifier: AmplifierPlatform::new(&root.join("dev/i2c-0")),
        }
    }
    fn path(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }
}

impl<C: Commands> Platform for LinuxPlatform<C> {
    fn read(&mut self, path: &str) -> Result<String, Error> {
        fs::read_to_string(self.path(path)).map_err(|error| Error::io(error, path))
    }
    fn write(&mut self, path: &str, value: &str) -> Result<(), Error> {
        let file = File::create(self.path(path)).map_err(|error| Error::io(error, path))?;
        let result = (&file).write_all(value.as_bytes());
        nix::unistd::close(file).map_err(|error| Error::io(error.into(), path))?;
        result.map_err(|error| Error::io(error, path))
    }
    fn command(&mut self, command: &Command) -> Result<CommandOutput, Error> {
        self.commands.run(command)
    }
    fn print(&mut self, value: &str) -> Result<(), Error> {
        writeln!(io::stdout().lock(), "{value}").map_err(|error| Error::io(error, "stdout"))
    }
    fn sleep(&mut self, seconds: f64) -> Result<(), Error> {
        let duration = Duration::try_from_secs_f64(seconds)
            .map_err(|error| Error::Other(error.to_string()))?;
        std::thread::sleep(duration);
        Ok(())
    }
    fn monotonic(&mut self) -> Result<f64, Error> {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        Ok(Duration::try_from(time)
            .map_err(|error| Error::Other(error.to_string()))?
            .as_secs_f64())
    }
    fn touch(&mut self, path: &str) -> Result<(), Error> {
        let target = self.path(path);
        let now = rustix::fs::Timestamps {
            last_access: rustix::time::Timespec {
                tv_sec: 0,
                tv_nsec: rustix::fs::UTIME_NOW,
            },
            last_modification: rustix::time::Timespec {
                tv_sec: 0,
                tv_nsec: rustix::fs::UTIME_NOW,
            },
        };
        if rustix::fs::utimensat(rustix::fs::CWD, &target, &now, rustix::fs::AtFlags::empty())
            .is_err()
        {
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(target)
                .map_err(|error| Error::io(error, path))?;
        }
        Ok(())
    }
    fn sync(&mut self) -> Result<(), Error> {
        rustix::fs::sync();
        Ok(())
    }
    fn c3x_lite(&mut self) -> Result<bool, Error> {
        let params = if self.root == Path::new("/") {
            openpilot_params::Params::for_runtime()
        } else {
            openpilot_params::Params::open(&self.path("/data/params"), "d")
        }
        .map_err(|error| Error::Other(error.to_string()))?;
        params
            .get_bool("HardwareC3xLite")
            .map_err(|error| Error::Other(error.to_string()))
    }
    fn amplifier(&mut self, action: AmplifierAction<'_>) -> Result<bool, Error> {
        let amplifier = Amplifier::new(false);
        let result = match action {
            AmplifierAction::Shutdown(disabled) => {
                amplifier.set_global_shutdown(&mut self.amplifier, disabled)
            }
            AmplifierAction::Initialize(model) => {
                amplifier.initialize_configuration(&mut self.amplifier, model)
            }
        };
        result.map_err(|error| Error::Other(error.to_string()))
    }
}

fn launch_error(error: openpilot_process_supervision::Error) -> Error {
    match error {
        openpilot_process_supervision::Error::Io(source) => Error::io(source, "command"),
        error => Error::Other(error.to_string()),
    }
}
