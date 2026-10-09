use super::{paths, Failure, Service};
use crate::{Error, Value};
use openpilot_dashcam_upload::{catalog, manager::Manager, worker::Settings};
use std::{path::PathBuf, sync::Arc, thread};
use tokio::sync::{mpsc, oneshot};

enum Operation {
    Start(Vec<String>),
    Snapshot(String),
    Cancel(String),
}
struct Command {
    operation: Operation,
    response: oneshot::Sender<Result<serde_json::Value, openpilot_dashcam_upload::Error>>,
}
struct Controller {
    commands: Option<mpsc::Sender<Command>>,
    owner: Option<thread::JoinHandle<()>>,
}
impl Drop for Controller {
    fn drop(&mut self) {
        drop(self.commands.take());
        if let Some(owner) = self.owner.take() {
            if owner.join().is_err() {
                eprintln!("dashcam upload manager thread failed");
            }
        }
    }
}
enum Owner {
    Running(Controller),
    Unavailable(String),
}
pub struct Uploads {
    root: PathBuf,
    owner: Owner,
}
fn failure(error: openpilot_dashcam_upload::Error) -> Failure {
    match error {
        openpilot_dashcam_upload::Error::Http { status, text } => Failure::Http {
            status,
            message: text,
        },
        error => Error::Source(error.to_string()).into(),
    }
}
fn value(value: &serde_json::Value) -> Result<Value, Failure> {
    Value::parse(&serde_json::to_string(value).map_err(|error| Error::Source(error.to_string()))?)
        .map_err(Error::from)
        .map_err(Failure::from)
}
impl Uploads {
    pub fn original(service: &Service) -> Arc<Self> {
        let executable = std::env::current_exe().map_or_else(
            |_| PathBuf::from("openpilot-dashcam-upload"),
            |path| path.with_file_name("openpilot-dashcam-upload"),
        );
        Self::for_test(service.root.clone(), executable, None)
    }
    pub fn for_test(root: PathBuf, executable: PathBuf, settings: Option<Settings>) -> Arc<Self> {
        let (commands, mut receiver) = mpsc::channel::<Command>(16);
        let worker_root = root.clone();
        let owner = match thread::Builder::new()
            .name("dashcam-upload-manager".into())
            .spawn(move || {
                let mut manager = Manager::new(executable);
                while let Some(command) = receiver.blocking_recv() {
                    let result = match command.operation {
                        Operation::Start(segments) => {
                            manager.start(&worker_root, &segments, settings.clone())
                        }
                        Operation::Snapshot(id) => manager.snapshot(&id),
                        Operation::Cancel(id) => manager.cancel(&id),
                    };
                    if command.response.send(result).is_err() {
                        eprintln!("dashcam upload response receiver closed");
                    }
                }
            }) {
            Ok(owner) => Owner::Running(Controller {
                commands: Some(commands),
                owner: Some(owner),
            }),
            Err(error) => Owner::Unavailable(error.to_string()),
        };
        Arc::new(Self { root, owner })
    }
    async fn request(&self, operation: Operation) -> Result<Value, Failure> {
        let controller = match &self.owner {
            Owner::Running(controller) => controller,
            Owner::Unavailable(error) => return Err(Error::Source(error.clone()).into()),
        };
        let commands = controller
            .commands
            .as_ref()
            .ok_or_else(|| Error::Source("dashcam upload manager stopped".into()))?;
        let (response, result) = oneshot::channel();
        commands
            .send(Command {
                operation,
                response,
            })
            .await
            .map_err(|_| Error::Source("dashcam upload manager stopped".into()))?;
        let result = result
            .await
            .map_err(|_| Error::Source("dashcam upload manager stopped".into()))?
            .map_err(failure)?;
        value(&result)
    }
    pub(super) fn summary(&self, segments: &[String]) -> Result<Value, Failure> {
        let segments = catalog::validate_selection(&self.root, segments).map_err(failure)?;
        let mut summaries = Vec::with_capacity(segments.len());
        for segment in segments {
            let directory = catalog::segment_dir(&self.root, &segment).map_err(failure)?;
            let files = catalog::file_summary(&directory).map_err(failure)?;
            let total = files.iter().try_fold(0_u64, |total, file| {
                total
                    .checked_add(file.size)
                    .ok_or_else(|| Error::Source("upload size overflow".into()))
            })?;
            let segment = Value::text(&segment);
            summaries.push(Value::object([
                ("segment", segment.clone()),
                ("route", paths::route_name(&segment)?),
                (
                    "segmentIndex",
                    Value::Integer(paths::segment_index(&segment)),
                ),
                (
                    "files",
                    value(
                        &serde_json::to_value(files)
                            .map_err(|error| Error::Source(error.to_string()))?,
                    )?,
                ),
                ("totalSize", Value::integer(total)),
                ("totalSizeLabel", Value::text(&catalog::size_label(total))),
            ]));
        }
        Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("summaries", Value::Array(summaries)),
        ]))
    }
    pub(super) async fn start(&self, segments: Vec<String>) -> Result<Value, Failure> {
        self.request(Operation::Start(segments)).await
    }
    pub(super) async fn snapshot(&self, id: String) -> Result<Value, Failure> {
        self.request(Operation::Snapshot(id)).await
    }
    pub(super) async fn cancel(&self, id: String) -> Result<Value, Failure> {
        self.request(Operation::Cancel(id)).await
    }
}
