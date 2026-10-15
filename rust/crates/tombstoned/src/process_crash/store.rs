//! Bounded persistent evidence; capture and network calls stay outside this lock.
use openpilot_logging::{Fields, Value};
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

pub const MAX_TEXT: usize = 256 * 1024;
pub const MAX_REPORTS: usize = 20;
pub const RETRY_SECONDS: f64 = 60.0;

pub fn fields(bytes: &[u8]) -> io::Result<Fields> {
    match serde_json::from_slice::<Value>(bytes).map_err(io::Error::other)? {
        Value::Object(value) => Ok(value),
        _ => Err(io::Error::other("crash record must be an object")),
    }
}
pub fn text<'a>(event: &'a Fields, key: &str) -> Option<&'a str> {
    match event.get(key) {
        Some(Value::Text(value)) => Some(value),
        _ => None,
    }
}
fn integer(value: Option<&Value>) -> Option<i128> {
    match value {
        Some(Value::Integer(v)) => Some(*v),
        Some(Value::Bool(v)) => Some(i128::from(*v)),
        _ => None,
    }
}
fn signal(code: i128) -> &'static str {
    match code {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        5 => "SIGTRAP",
        6 => "SIGABRT",
        7 => "SIGBUS",
        8 => "SIGFPE",
        9 => "SIGKILL",
        10 => "SIGUSR1",
        11 => "SIGSEGV",
        12 => "SIGUSR2",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        16 => "SIGSTKFLT",
        17 => "SIGCHLD",
        18 => "SIGCONT",
        19 => "SIGSTOP",
        20 => "SIGTSTP",
        21 => "SIGTTIN",
        22 => "SIGTTOU",
        23 => "SIGURG",
        24 => "SIGXCPU",
        25 => "SIGXFSZ",
        26 => "SIGVTALRM",
        27 => "SIGPROF",
        28 => "SIGWINCH",
        29 => "SIGIO",
        30 => "SIGPWR",
        31 => "SIGSYS",
        34 => "SIGRTMIN",
        64 => "SIGRTMAX",
        _ => "unknown",
    }
}
fn valid_id(id: &str) -> bool {
    let parts: Vec<_> = id.split('-').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty() && part.len() <= 20 && part.bytes().all(|b| b.is_ascii_digit())
        })
}
pub fn validate(event: &Fields) -> io::Result<()> {
    if !text(event, "id").is_some_and(valid_id) {
        return Err(io::Error::other("invalid crash id"));
    }
    let name = match event.get("name") {
        Some(Value::Text(value)) => Some(value.clone()),
        Some(Value::Integer(value)) => Some(value.to_string()),
        Some(Value::Bool(value)) => Some(if *value { "True" } else { "False" }.into()),
        Some(Value::Null) => Some("None".into()),
        _ => None,
    };
    if !name.is_some_and(|name| {
        !name.is_empty()
            && name.len() <= 100
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }) {
        return Err(io::Error::other("invalid process name"));
    }
    if integer(event.get("pid")).is_none() || integer(event.get("exitcode")).is_none() {
        return Err(io::Error::other("invalid process status"));
    }
    Ok(())
}
pub fn fallback(event: &Fields) -> Fields {
    ["git_branch", "git_commit", "dongle_id"]
        .into_iter()
        .map(|key| {
            (
                key.to_owned(),
                event
                    .get(key)
                    .cloned()
                    .unwrap_or(Value::Text(String::new())),
            )
        })
        .chain([("tmux_why".into(), Value::Text("process_crash".into()))])
        .collect()
}
pub fn tail(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(MAX_TEXT)..]).into_owned()
}
pub struct Prepared {
    pub event: Fields,
    pub payload: Fields,
    pub text: String,
}
impl Prepared {
    pub fn capture(
        mut event: Fields,
        now: f64,
        capture: impl FnOnce(&Fields) -> io::Result<String>,
        payload: impl FnOnce(&Fields) -> io::Result<Fields>,
    ) -> io::Result<Self> {
        validate(&event)?;
        if let Some(code) = integer(event.get("exitcode")).filter(|code| *code < 0) {
            event.insert(
                "signal".into(),
                Value::Text(signal(code.saturating_neg()).into()),
            );
        }
        event.insert("captured_at".into(), Value::Float(now));
        let captured = capture(&event)?;
        let payload = payload(&event).unwrap_or_else(|_| fallback(&event));
        Ok(Self {
            event,
            payload,
            text: tail(captured.as_bytes()),
        })
    }
}
pub struct Store {
    pub root: PathBuf,
    pub retry_at: HashMap<String, f64>,
}
impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            retry_at: HashMap::new(),
        }
    }
    pub fn record(&mut self, prepared: Prepared) -> io::Result<()> {
        let id = text(&prepared.event, "id").ok_or_else(|| io::Error::other("missing crash id"))?;
        // Revalidate at the filesystem boundary, including public callers.
        validate(&prepared.event)?;
        fs::create_dir_all(&self.root)?;
        let path = self.root.join(format!("{id}.json"));
        if path.exists() {
            return Ok(());
        }
        let metadata = prepared.event.to_json().map_err(io::Error::other)?;
        fs::write(
            path.with_extension("txt"),
            format!("PROCESS_EXIT {metadata}\n\n{}", prepared.text),
        )?;
        let report = [
            ("event".into(), Value::Object(prepared.event)),
            ("payload".into(), Value::Object(prepared.payload)),
            ("sent".into(), Value::Array(Vec::new())),
        ]
        .into_iter()
        .collect();
        atomic(&path, &report)?;
        let mut reports = self.paths()?;
        reports.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        let remove = reports.len().saturating_sub(MAX_REPORTS);
        for path in reports.into_iter().take(remove) {
            match fs::remove_file(path.with_extension("txt")) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            fs::remove_file(&path)?;
            if let Some(name) = path.file_name().and_then(|v| v.to_str()) {
                self.retry_at.remove(name);
            }
        }
        Ok(())
    }
    fn paths(&self) -> io::Result<Vec<PathBuf>> {
        let entries = match fs::read_dir(&self.root) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        entries
            .map(|e| e.map(|e| e.path()))
            .filter(|p| !matches!(p,Ok(p)if p.extension().is_none_or(|x|x!="json")))
            .collect()
    }
    pub fn due(&self, now: f64) -> io::Result<Vec<PathBuf>> {
        Ok(self
            .paths()?
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|v| v.to_str())
                    .is_some_and(|name| now >= *self.retry_at.get(name).unwrap_or(&0.0))
            })
            .collect())
    }
    pub fn acknowledged(&self, path: &Path, target: &str) -> io::Result<bool> {
        let report = fields(&fs::read(path)?)?;
        match report.get("sent") {
            Some(Value::Array(sent)) => {
                Ok(sent.iter().any(|v| matches!(v,Value::Text(t)if t==target)))
            }
            _ => Err(io::Error::other("invalid sent destinations")),
        }
    }
    pub fn acknowledge(&mut self, path: &Path, target: &str) -> io::Result<()> {
        let bytes = match fs::read(path) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut report = fields(&bytes)?;
        let Some(Value::Array(sent)) = report.get_mut("sent") else {
            return Err(io::Error::other("invalid sent destinations"));
        };
        if !sent.iter().any(|v| matches!(v,Value::Text(t)if t==target)) {
            sent.push(Value::Text(target.into()));
        }
        atomic(path, &report)
    }
    pub fn defer(&mut self, path: &Path, now: f64) {
        if path.exists() {
            if let Some(name) = path.file_name().and_then(|v| v.to_str()) {
                self.retry_at.insert(name.into(), now + RETRY_SECONDS);
            }
        }
    }
}
fn atomic(path: &Path, value: &Fields) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, value.to_json().map_err(io::Error::other)?)?;
    fs::rename(temporary, path)
}
