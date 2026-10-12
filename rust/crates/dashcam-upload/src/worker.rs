use crate::{
    metadata,
    state::{Clock, Finish, Progress},
    Error,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    env,
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub root: PathBuf,
    pub base_url: String,
    pub token: String,
    pub metadata: Value,
    pub webhook: String,
    pub concurrency: usize,
}
impl Settings {
    pub fn for_runtime(logger: &mut openpilot_logging::producer::Logger) -> Result<Self, Error> {
        let params = openpilot_params::Params::for_runtime()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        let environment = [
            "CARROT_DEVICE_SERIAL",
            "DEVICE_SERIAL",
            "SERIAL",
            "CARROT_DISCORD_WEBHOOK_URL",
            "DISCORD_WEBHOOK_URL",
            "CARROT_DISCORD_WEBHOOK_DISABLE",
            "CARROT_WEB_UPLOAD_CONCURRENCY",
        ]
        .into_iter()
        .map(|key| {
            let value = match env::var(key) {
                Ok(value) => value,
                Err(env::VarError::NotPresent) => String::new(),
                Err(error) => return Err(Error::Runtime(error.to_string())),
            };
            Ok((key.to_owned(), value))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let upload_environment = openpilot_web_upload::Environment::for_runtime()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        let (repo, settings) = metadata::runtime_paths();
        let (base_url, token) = metadata::target_settings(&settings, &upload_environment)?;
        Ok(Self {
            root: "/data/media/0/realdata".into(),
            base_url,
            token,
            metadata: metadata::upload_metadata(
                Some(&params),
                &repo,
                &environment,
                &metadata::hardware_serial(),
                logger,
            ),
            webhook: metadata::webhook_url(Some(&params), &environment, logger),
            concurrency: concurrency(
                environment
                    .get("CARROT_WEB_UPLOAD_CONCURRENCY")
                    .map(String::as_str),
            ),
        })
    }
}
pub fn concurrency(value: Option<&str>) -> usize {
    let value = value.filter(|s| !s.is_empty()).unwrap_or("3").trim();
    let negative = value.starts_with('-');
    let value = value.strip_prefix(['+', '-']).unwrap_or(value);
    let (mut number, mut digits, mut needs_digit) = (0usize, 0usize, true);
    for character in value.chars() {
        if character == '_' && !needs_digit {
            needs_digit = true;
        } else if let Some(digit) = crate::catalog::decimal_digit(character) {
            number = (number * 10 + usize::from(digit)).min(6);
            digits += 1;
            needs_digit = false;
        } else {
            return 3;
        }
    }
    if needs_digit || digits > 4300 {
        3
    } else if negative {
        1
    } else {
        number.max(1)
    }
}
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub parent_pid: u32,
    pub root: PathBuf,
    pub id: String,
    pub segments: Vec<String>,
    pub settings: Option<Settings>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Touch,
    Append {
        text: String,
    },
    Progress {
        patch: Progress,
    },
    Context {
        metadata: Value,
        remote_base_path: String,
    },
    Partial {
        results: Vec<Value>,
    },
    Finish {
        patch: Finish,
    },
}
#[derive(Serialize, Deserialize)]
pub struct Packet {
    pub clock: Clock,
    #[serde(flatten)]
    pub event: Event,
}
pub fn clock() -> Clock {
    let wall = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |time| time.as_secs_f64());
    let mono = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    Clock {
        wall,
        monotonic: mono.tv_sec as f64 + mono.tv_nsec as f64 * 1e-9,
    }
}
#[derive(Clone)]
pub(crate) struct Output(Arc<Mutex<io::Stdout>>);
impl Output {
    pub(crate) fn emit(&self, event: Event) -> Result<(), Error> {
        let mut writer = self
            .0
            .lock()
            .map_err(|error| Error::Runtime(error.to_string()))?;
        serde_json::to_writer(
            &mut *writer,
            &Packet {
                clock: clock(),
                event,
            },
        )?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }
}
pub fn run() -> Result<(), Error> {
    rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::KILL))
        .map_err(std::io::Error::from)?;
    let mut first = String::new();
    io::stdin().read_line(&mut first)?;
    let request: Request = serde_json::from_str(&first)?;
    if rustix::process::getppid().map(|pid| pid.as_raw_pid() as u32) != Some(request.parent_pid) {
        return Err(Error::Runtime("upload manager no longer exists".into()));
    }
    let output = Output(Arc::new(Mutex::new(io::stdout())));
    let canceled = Arc::new(AtomicBool::new(false));
    let control = canceled.clone();
    thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if line.is_ok_and(|line| line == "cancel") {
                control.store(true, Ordering::Release);
            }
        }
    });
    let mut logger = openpilot_logging::producer::Factory::for_runtime()?.logger();
    let result = request
        .settings
        .map_or_else(|| Settings::for_runtime(&mut logger), Ok)
        .and_then(|mut settings| {
            settings.root = request.root;
            crate::engine::run(settings, request.segments, canceled, output.clone())
        });
    match result {
        Ok(patch) => output.emit(Event::Finish { patch }),
        Err(error) => {
            output.emit(Event::Append {
                text: format!("FAILED: {error}"),
            })?;
            output.emit(Event::Finish {
                patch: Finish {
                    ok: false,
                    result: Some(serde_json::json!({"ok":false,"error":error.to_string()})),
                    error: Some(error.to_string()),
                    status: None,
                },
            })
        }
    }
}
