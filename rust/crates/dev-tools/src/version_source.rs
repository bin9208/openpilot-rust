//! Independent source-side Version/Git policy for the native QA migration.
//! Policy comes from the retained original Python sources, not runtime-version
//! (the candidate). A Session owns virtual cwd, raw helper caches and child env.
use crate::qa_process::{CommandSpec, NativeRunner, Runner};
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs, io,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
mod metadata;
mod value;
use value as v;

type Result<T> = std::result::Result<T, Error>;
#[derive(Debug)]
pub struct Error {
    kind: &'static str,
    message: String,
}
impl Error {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn kind(&self) -> &str {
        self.kind
    }
    fn io(error: &io::Error) -> Self {
        let kind = match error.raw_os_error() {
            Some(2) => "FileNotFoundError",
            Some(20) => "NotADirectoryError",
            Some(21) => "IsADirectoryError",
            Some(13 | 1) => "PermissionError",
            Some(17) => "FileExistsError",
            _ => "OSError",
        };
        Self::new(kind, error.to_string())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(output, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}

const SOURCES: &[(&str, &str)] = &[
    (
        "rust/tools/check_version_reference.py",
        "d0ad5fbec23e1ba4a2507ae6dbc68856141d03eda07dadebab3280d5731525d5",
    ),
    (
        "openpilot/system/version.py",
        "69b91ce6fc8fe3b164c4f6c23e9514c89a4b6759043cfb607b733c7e006144d7",
    ),
    (
        "openpilot/common/git.py",
        "68a68daab8137ba307753b6f1dbc18a4a8c736ed118383800ad6bfd8690140d9",
    ),
    (
        "openpilot/common/utils.py",
        "b1cc68b63cff1386ba397c0cc527dbb38fb50b154043ffcb066ed7acce65ec83",
    ),
    (
        "rust/tools/check_version_collector.py",
        "4daebbf121dceef9aec57b0ba35ac7aea2b827ec85bc2cb49760b6ce0e2f9a22",
    ),
];
/// Retained inputs are mandatory. Only the retired collector's exact path may
/// be absent; its compiled historical identity remains in the complete roster.
pub fn sources(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut actual = BTreeMap::new();
    for &(name, expected) in SOURCES {
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            Err(error)
                if name == "rust/tools/check_version_collector.py"
                    && error.kind() == io::ErrorKind::NotFound =>
            {
                actual.insert(name.into(), expected.into());
                continue;
            }
            Err(error) => return Err(Error::io(&error)),
            Ok(_) => {}
        }
        let bytes = fs::read(path).map_err(|error| Error::io(&error))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if hash != expected {
            return Err(Error::new(
                "ValueError",
                format!("changed Version source: {name}"),
            ));
        }
        actual.insert(name.into(), hash);
    }
    Ok(actual)
}

pub struct Session {
    cwd: PathBuf,
    // Retain initial and admitted cwd identity across renames. Numeric chdir
    // must also outlive the caller's descriptor.
    _cwd_fd: Option<OwnedFd>,
    environment: BTreeMap<OsString, OsString>,
    cache: BTreeMap<(String, String), JsonValue>,
    logs: Vec<JsonValue>,
    control: Option<(Instant, Arc<AtomicBool>)>,
    stderr: Option<fs::File>,
}
impl Session {
    pub fn new(cwd: &Path, environment: &BTreeMap<OsString, OsString>) -> Result<Self> {
        let cwd = fs::canonicalize(cwd).map_err(|error| Error::io(&error))?;
        if !cwd.is_dir() {
            return Err(Error::new(
                "NotADirectoryError",
                "source cwd is not a directory",
            ));
        }
        let owned = rustix::fs::open(
            &cwd,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| Error::io(&io::Error::from(error)))?;
        Ok(Self {
            cwd: PathBuf::from(format!("/proc/self/fd/{}", owned.as_raw_fd())),
            _cwd_fd: Some(owned),
            environment: environment.clone(),
            cache: BTreeMap::new(),
            logs: Vec::new(),
            control: None,
            stderr: None,
        })
    }
    /// Route source Git diagnostics to the scenario's owned file.
    pub fn set_stderr(&mut self, stderr: fs::File) {
        self.stderr = Some(stderr);
    }
    /// The controller's deadline covers the whole source request, including all
    /// Git calls. Control failures escape source policy exception serialization.
    pub fn request_controlled(
        &mut self,
        raw_json: &[u8],
        deadline: Instant,
        cancelled: Arc<AtomicBool>,
    ) -> Result<JsonValue> {
        self.control = Some((deadline, cancelled));
        let result = self.check_control().and_then(|()| self.request(raw_json));
        let control = self.check_control();
        self.control = None;
        control.and(result)
    }
    fn check_control(&self) -> Result<()> {
        if let Some((deadline, cancelled)) = &self.control {
            if cancelled.load(Ordering::Relaxed) {
                return Err(Error::new("InterruptedError", "source request interrupted"));
            }
            if Instant::now() >= *deadline {
                return Err(Error::new("TimeoutError", "source request timed out"));
            }
        }
        Ok(())
    }
    /// Handle one source-worker JSON request. Caught exceptions are response
    /// values; unknown operations and out-of-range numeric chdir descriptors
    /// preserve the worker's uncaught AssertionError/OverflowError respectively.
    pub fn request(&mut self, raw_json: &[u8]) -> Result<JsonValue> {
        self.logs.clear();
        let result = std::str::from_utf8(raw_json)
            .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))
            .and_then(|source| v::parse(&source.chars().map(u32::from).collect::<Vec<_>>()))
            .and_then(|request| self.call(&request));
        self.response(result)
    }
    /// Already-decoded source transport text can contain Python stdin's raw
    /// surrogateescape codepoints. The strict UTF-8 request API stays unchanged.
    pub(crate) fn request_codepoints(&mut self, text: &[u32]) -> Result<JsonValue> {
        self.logs.clear();
        let result = v::parse(text).and_then(|request| self.call(&request));
        self.response(result)
    }
    fn response(&self, result: Result<JsonValue>) -> Result<JsonValue> {
        if let Err(error) = &result {
            if matches!(error.kind(), "AssertionError" | "OverflowError") {
                return Err(Error::new(error.kind, &error.message));
            }
        }
        let result = v::capture(result);
        let logs = v::array(&self.logs);
        match (result.get("value"), result.get("error")) {
            (Some(value), _) => Ok(v::object(&[("value", value), ("logs", logs)])),
            (_, Some(error)) => Ok(v::object(&[("error", error), ("logs", logs)])),
            _ => unreachable!("capture always returns a value or error"),
        }
    }
    fn call(&mut self, request: &JsonValue) -> Result<JsonValue> {
        let op = v::required(request, "op")?;
        if op.text_eq("metadata") {
            return self.metadata(&v::required(request, "path")?);
        }
        if op.text_eq("from_json") {
            let source = v::required(request, "source")?;
            return metadata::from_dict(&v::parse(&v::points(&source, "TypeError")?)?);
        }
        if op.text_eq("git") {
            let requested = v::required(request, "method")?;
            v::points(&requested, "TypeError")?;
            let method = [
                "commit",
                "commit_date",
                "short_branch",
                "branch",
                "origin",
                "normalized_origin",
            ]
            .into_iter()
            .find(|name| requested.text_eq(name))
            .ok_or_else(|| Error::new("AttributeError", "unknown Git helper"))?;
            let mut args = Vec::new();
            if let Some(cwd) = request.get("cwd") {
                args.push(cwd);
            }
            if ["commit", "commit_date"].contains(&method) {
                if args.is_empty() {
                    args.push(v::null());
                }
                args.push(
                    request
                        .get("revision")
                        .unwrap_or_else(|| JsonValue::text("HEAD")),
                );
            }
            return self.git_helper(method, &args);
        }
        let path = if ["version", "release_notes", "dirty", "prebuilt", "chdir"]
            .iter()
            .any(|name| op.text_eq(name))
        {
            v::required(request, "path")?
        } else {
            return Err(Error::new("AssertionError", "unknown source operation"));
        };
        if op.text_eq("version") {
            return self.version(&path);
        }
        if op.text_eq("release_notes") {
            return self.release_notes(&path);
        }
        if op.text_eq("dirty") {
            return self.dirty(&path);
        }
        if op.text_eq("prebuilt") {
            return self.prebuilt(&path);
        }
        // Unlike pathlib.Path/os.path.join consumers, os.chdir accepts C-int
        // file descriptors (including bool) and rejects an empty string.
        let descriptor = match path.view() {
            JsonView::Integer(text) => Some(
                text.parse::<i32>()
                    .map_err(|_| Error::new("OverflowError", "fd is outside C int range"))?,
            ),
            JsonView::Bool(value) => Some(i32::from(value)),
            _ => None,
        };
        let target = if let Some(descriptor) = descriptor {
            PathBuf::from(format!("/proc/self/fd/{descriptor}"))
        } else {
            if path.text_eq("") {
                return Err(Error::new("FileNotFoundError", "empty chdir path"));
            }
            self.path(&path)?
        };
        reject_nul(target.as_os_str())?;
        let owned = rustix::fs::open(
            &target,
            rustix::fs::OFlags::PATH | rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| {
            if descriptor.is_some() && error == rustix::io::Errno::NOENT {
                Error::new("OSError", "bad file descriptor")
            } else {
                Error::io(&io::Error::from(error))
            }
        })?;
        // O_PATH intentionally does not require read access. fchdir/chdir need
        // directory execute/search access, checked using the effective identity.
        rustix::fs::accessat(
            &owned,
            ".",
            rustix::fs::Access::EXEC_OK,
            rustix::fs::AtFlags::EACCESS,
        )
        .map_err(|error| Error::io(&io::Error::from(error)))?;
        self.cwd = PathBuf::from(format!("/proc/self/fd/{}", owned.as_raw_fd()));
        self._cwd_fd = Some(owned);
        Ok(v::null())
    }
    fn path(&self, value: &JsonValue) -> Result<PathBuf> {
        let path = PathBuf::from(v::os_string(value)?);
        Ok(if path.is_absolute() {
            path
        } else {
            self.cwd.join(path)
        })
    }
    fn git_cwd(&self, args: &[JsonValue]) -> Result<PathBuf> {
        match args.first() {
            None => Ok(self.cwd.clone()),
            Some(value) if matches!(value.view(), JsonView::Null) => Ok(self.cwd.clone()),
            Some(value) if value.text_eq("") => {
                Err(Error::new("FileNotFoundError", "empty subprocess cwd"))
            }
            Some(value) => self.path(value),
        }
    }
    fn command(&self, args: &[OsString], cwd: &Path) -> Result<crate::qa_process::Captured> {
        self.check_control()?;
        reject_nul(cwd.as_os_str())?;
        for argument in args {
            reject_nul(argument)?;
        }
        let mut spec = CommandSpec::new_os(
            std::iter::once(OsString::from("git"))
                .chain(args.iter().cloned())
                .collect(),
            Duration::from_secs(30),
        );
        spec.cwd = Some(cwd.into());
        // Source check_output buffers stdout without a size threshold and leaves
        // Git diagnostics on the source worker's stderr stream.
        spec.stdout_limit = usize::MAX;
        spec.inherit_stderr = true;
        spec.set_os_env(self.environment.clone());
        spec.env_remove = std::env::vars_os()
            .filter_map(|(key, _)| (!self.environment.contains_key(&key)).then_some(key))
            .collect();
        let control = self
            .control
            .as_ref()
            .map(|(deadline, cancelled)| (*deadline, cancelled.as_ref()));
        let result = if let Some(stderr) = &self.stderr {
            NativeRunner.run_with_stderr(&spec, control, stderr)
        } else if let Some((deadline, cancelled)) = control {
            NativeRunner.run_controlled(&spec, deadline, cancelled)
        } else {
            NativeRunner.run(&spec)
        };
        self.check_control()?;
        result.map_err(|error| {
            if let Some(error) = error.downcast_ref::<io::Error>() {
                Error::io(error)
            } else {
                Error::new("OSError", error.to_string())
            }
        })
    }
    fn run_git(&self, args: &[OsString], cwd: &Path) -> Result<JsonValue> {
        let result = self.command(args, cwd)?;
        // check_output(text=True) decodes communicate() before checking status.
        let text = std::str::from_utf8(&result.stdout)
            .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))?;
        if result.code != 0 {
            return Err(Error::new(
                "CalledProcessError",
                format!("Git exited {}", result.code),
            ));
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        Ok(JsonValue::text(text.trim_matches(|c: char| {
            c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
        })))
    }
    fn default_git(&self, args: &[OsString], cwd: &Path) -> Result<JsonValue> {
        match self.run_git(args, cwd) {
            Err(error) if error.kind() == "CalledProcessError" => Ok(JsonValue::text("")),
            result => result,
        }
    }
    fn git_helper(&mut self, method: &str, args: &[JsonValue]) -> Result<JsonValue> {
        let key = (format!("git:{method}"), v::cache_key(args)?);
        if let Some(value) = self.cache.get(&key) {
            return Ok(value.clone());
        }
        let cwd = self.git_cwd(args)?;
        let static_args = |args: &[&str]| {
            args.iter()
                .map(|arg| OsString::from(*arg))
                .collect::<Vec<_>>()
        };
        let value = match method {
            "commit" | "commit_date" => {
                let revision = args
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| JsonValue::text("HEAD"));
                let revision = v::os_string(&revision)?;
                let mut command = if method == "commit" {
                    static_args(&["rev-parse"])
                } else {
                    static_args(&["show", "--no-patch", "--format='%ct %ci'"])
                };
                command.push(revision);
                self.default_git(&command, &cwd)?
            }
            "short_branch" => {
                self.default_git(&static_args(&["rev-parse", "--abbrev-ref", "HEAD"]), &cwd)?
            }
            "branch" => self.default_git(
                &static_args(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]),
                &cwd,
            )?,
            "origin" => {
                let tracking = self
                    .run_git(&static_args(&["name-rev", "--name-only", "HEAD"]), &cwd)
                    .and_then(|branch| {
                        let branch = branch.to_utf8().expect("strict UTF-8 Git output");
                        self.run_git(
                            &["config".into(), format!("branch.{branch}.remote").into()],
                            &cwd,
                        )
                    })
                    .and_then(|remote| {
                        let remote = remote.to_utf8().expect("strict UTF-8 Git output");
                        self.run_git(
                            &["config".into(), format!("remote.{remote}.url").into()],
                            &cwd,
                        )
                    });
                match tracking {
                    Err(error) if error.kind() == "CalledProcessError" => self.default_git(
                        &static_args(&["config", "--get", "remote.origin.url"]),
                        &cwd,
                    )?,
                    result => result?,
                }
            }
            "normalized_origin" => {
                let cwd_arg = args.first().cloned().unwrap_or_else(v::null);
                v::normalized_origin(&self.git_helper("origin", &[cwd_arg])?)?
            }
            _ => return Err(Error::new("AttributeError", method)),
        };
        self.cache.insert(key, value.clone());
        Ok(value)
    }
    fn read_text(&self, path: &Path) -> Result<String> {
        reject_nul(path.as_os_str())?;
        let bytes = fs::read(path).map_err(|error| Error::io(&error))?;
        let text = String::from_utf8(bytes)
            .map_err(|error| Error::new("UnicodeDecodeError", error.to_string()))?;
        Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
    }
    fn version(&self, path: &JsonValue) -> Result<JsonValue> {
        let text = self.read_text(&self.path(path)?.join("openpilot/common/version.h"))?;
        text.split('"')
            .nth(1)
            .map(JsonValue::text)
            .ok_or_else(|| Error::new("IndexError", "list index out of range"))
    }
    fn release_notes(&self, path: &JsonValue) -> Result<JsonValue> {
        let text = self.read_text(&self.path(path)?.join("RELEASES.md"))?;
        Ok(JsonValue::text(
            text.split_once("\n\n")
                .map_or(text.as_str(), |(first, _)| first),
        ))
    }
    fn prebuilt(&mut self, path: &JsonValue) -> Result<JsonValue> {
        let key = ("prebuilt".into(), v::cache_key(std::slice::from_ref(path))?);
        if let Some(value) = self.cache.get(&key) {
            return Ok(value.clone());
        }
        let exists = match self.path(path) {
            Ok(path) => fs::metadata(path.join("prebuilt")).is_ok(),
            Err(error) if error.kind() == "UnicodeEncodeError" => false,
            Err(error) => return Err(error),
        };
        let value = v::boolean(exists);
        self.cache.insert(key, value.clone());
        Ok(value)
    }
    fn dirty(&mut self, path: &JsonValue) -> Result<JsonValue> {
        let key = ("dirty".into(), v::cache_key(std::slice::from_ref(path))?);
        if let Some(value) = self.cache.get(&key) {
            return Ok(value.clone());
        }
        let origin = self.git_helper("origin", &[])?;
        let branch = if origin.text_eq("") {
            None
        } else {
            Some(self.git_helper("short_branch", &[])?)
        };
        let dirty = if branch.as_ref().is_none_or(|branch| branch.text_eq("")) {
            true
        } else if matches!(self.prebuilt(path)?.view(), JsonView::Bool(true)) {
            false
        } else {
            let branch = self.git_helper("branch", &[])?;
            if branch.text_eq("") {
                true
            } else {
                let args = vec![
                    "--no-optional-locks".into(),
                    "diff".into(),
                    "--quiet".into(),
                    v::os_string(&branch)?,
                    "--".into(),
                ];
                self.command(&args, &self.path(path)?)?.code != 0
            }
        };
        let value = v::boolean(dirty);
        self.cache.insert(key, value.clone());
        Ok(value)
    }
    fn metadata(&mut self, path: &JsonValue) -> Result<JsonValue> {
        let root = match self.path(path) {
            Ok(root) => root,
            Err(error) if error.kind() == "UnicodeEncodeError" => {
                return self.unavailable_metadata()
            }
            Err(error) => return Err(error),
        };
        if path_exists(&root.join("build.json"))? {
            let text = self.read_text(&root.join("build.json"))?;
            return metadata::from_dict(&v::parse(
                &text.chars().map(u32::from).collect::<Vec<_>>(),
            )?);
        }
        if path_exists(&root.join(".git"))? {
            let channel = self.git_helper("short_branch", std::slice::from_ref(path))?;
            let version = self.version(path)?;
            let notes = self.release_notes(path)?;
            let commit = self.git_helper("commit", std::slice::from_ref(path))?;
            let origin = self.git_helper("origin", std::slice::from_ref(path))?;
            let date = self.git_helper("commit_date", std::slice::from_ref(path))?;
            let dirty = self.dirty(path)?;
            let openpilot = v::object(&[
                ("version", version),
                ("release_notes", notes),
                ("git_commit", commit),
                ("git_origin", origin),
                ("git_commit_date", date),
                ("build_style", JsonValue::text("unknown")),
                ("is_dirty", dirty),
            ]);
            return Ok(metadata::build(channel, openpilot));
        }
        self.unavailable_metadata()
    }
    fn unavailable_metadata(&mut self) -> Result<JsonValue> {
        self.logs.push(v::object(&[
            ("msg", JsonValue::text("unable to get build metadata")),
            ("levelnum", JsonValue::parse("40").unwrap()),
            ("exc_info", JsonValue::text("NoneType: None\n")),
        ]));
        Err(Error::new("Exception", "invalid build metadata"))
    }
}
fn reject_nul(value: &std::ffi::OsStr) -> Result<()> {
    if value.as_bytes().contains(&0) {
        Err(Error::new("ValueError", "embedded null byte"))
    } else {
        Ok(())
    }
}
fn path_exists(path: &Path) -> Result<bool> {
    if path.as_os_str().as_bytes().contains(&0) {
        return Ok(false);
    }
    match fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if matches!(error.raw_os_error(), Some(2 | 20 | 40)) => Ok(false),
        Err(error) => Err(Error::io(&error)),
    }
}
