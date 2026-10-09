use openpilot_carrot_server::{
    config::Config,
    git_status::{Repository, Service as GitStatus},
    heartbeat::Service,
    http::{serve, Application},
    Error,
};
use serde::Deserialize;
use std::{fs::File, io, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Constructor {
    #[default]
    New,
    WithGitStatus,
    Runtime,
}

pub struct Input {
    pub root: PathBuf,
    pub params: Option<openpilot_params::Params>,
    pub service: Arc<Service>,
    pub constructor: Constructor,
    pub invalid_web: bool,
}

pub struct Running {
    pub active: bool,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<(), Error>>>,
    pressure: Option<Pressure>,
    probe: PathBuf,
}

struct Pressure {
    files: Vec<File>,
    previous: Option<rustix::process::Rlimit>,
}

impl Pressure {
    fn release(&mut self) -> io::Result<()> {
        self.files.clear();
        if let Some(previous) = self.previous.take() {
            rustix::process::setrlimit(rustix::process::Resource::Nofile, previous)?;
        }
        Ok(())
    }
}

impl Drop for Pressure {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

impl Running {
    pub fn start(input: Input, listener: TcpListener) -> Result<Self, Error> {
        let root = input.root;
        let mut config = Config::at(&root, &root.join("data"), &root.join("settings.json"));
        config.web = root.join(if input.invalid_web {
            "missing-web"
        } else {
            "web"
        });
        config.shared_assets = root.join("assets");
        config.training_assets = root.join("assets/training");
        config.legacy_state = root.join("legacy");
        config.params_backup = root.join("params-backup.json");
        let git_status = || {
            GitStatus::with_clock(
                Repository {
                    directory: root.clone(),
                    lock: root.join("unused-git-lock"),
                    launcher: root.join("unused-git-launcher"),
                },
                || 0.0,
            )
        };
        let backend = super::backend(&input.params, &config.state);
        let mut app = match input.constructor {
            Constructor::New => Application::new(config, backend),
            Constructor::WithGitStatus => {
                Application::with_git_status(config, backend, git_status())
            }
            Constructor::Runtime => Application::for_runtime(config, backend, git_status()),
        };
        let application = Arc::get_mut(&mut app)
            .ok_or_else(|| Error::Source("owned App already shared".into()))?;
        application.heartbeat = input.service;
        application.git_status = None;
        application.popular_values = openpilot_carrot_server::popular_values::Service::new(false);
        let active = application.heartbeat_params.is_some();
        let (stop, stopped) = oneshot::channel();
        let task = tokio::task::spawn_local(async move {
            serve(app, listener, async {
                let _ = stopped.await;
            })
            .await
        });
        let probe = root.join("descriptor-probe");
        std::fs::write(&probe, b"owned heartbeat accept fixture")?;
        Ok(Self {
            active,
            stop: Some(stop),
            task: Some(task),
            pressure: None,
            probe,
        })
    }

    pub fn limit_accept(&mut self) -> io::Result<()> {
        use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
        let previous = getrlimit(Resource::Nofile);
        setrlimit(
            Resource::Nofile,
            Rlimit {
                current: Some(128),
                maximum: previous.maximum,
            },
        )?;
        let mut pressure = Pressure {
            files: Vec::new(),
            previous: Some(previous),
        };
        loop {
            match File::open(&self.probe) {
                Ok(file) => pressure.files.push(file),
                Err(error)
                    if error.raw_os_error() == Some(rustix::io::Errno::MFILE.raw_os_error()) =>
                {
                    break
                }
                Err(error) => return Err(error),
            }
        }
        self.pressure = Some(pressure);
        Ok(())
    }

    pub async fn cleanup(&mut self) -> Result<Option<String>, Box<dyn std::error::Error>> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let result = self.task.take().ok_or("owned App already cleaned")?.await?;
        if let Some(mut pressure) = self.pressure.take() {
            pressure.release()?;
        }
        Ok(result.err().map(|error| error.to_string()))
    }

    pub fn cleaned(&self) -> bool {
        self.task.is_none()
    }
}
