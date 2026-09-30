use crate::{
    apport::Retrace,
    discovery::{clear_apport_folder, get_tombstones, Tombstone},
    parse::safe_fn,
    reader, Error, MAX_SIZE, MAX_TOMBSTONE_FN_LEN,
};
use openpilot_crash_reporting::{Inputs, Project, Reporter, Sdk};
use openpilot_logging::{
    log_site,
    record::{Level, Record},
    PythonText,
};
use openpilot_logmessaged::{JsonValue, JsonView};
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

fn path_points(path: &Path) -> Vec<u32> {
    let mut bytes = path.as_os_str().as_encoded_bytes();
    let mut points = Vec::new();
    while !bytes.is_empty() {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                points.extend(text.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                points.extend(String::from_utf8_lossy(valid).chars().map(u32::from));
                let length = error.error_len().unwrap_or(rest.len());
                points.extend(rest[..length].iter().map(|byte| 0xdc00 + u32::from(*byte)));
                bytes = &rest[length..];
            }
        }
    }
    points
}
fn path_record(level: Level, prefix: &str, path: &Path, suffix: &str) -> Result<Record, Error> {
    let points = prefix
        .chars()
        .map(u32::from)
        .chain(path_points(path))
        .chain(suffix.chars().map(u32::from))
        .collect();
    Ok(Record::python_text(level, PythonText::new(points)?))
}

pub trait Clock {
    fn local_stamp(&mut self) -> String;
}
pub struct WallClock;
impl Clock for WallClock {
    fn local_stamp(&mut self) -> String {
        chrono::Local::now()
            .format("%Y-%m-%d--%H-%M-%S")
            .to_string()
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ReportOutcome {
    TooLarge,
    Copied(PathBuf),
}
fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}
fn commit_prefix(value: &JsonValue) -> Result<String, Error> {
    let value = if truth(value) {
        value.clone()
    } else {
        JsonValue::text("nocommit")
    };
    let prefix = match value.view() {
        JsonView::Text(points) => JsonValue::codepoints(points.iter().take(8).copied().collect())
            .ok_or(Error::Contract("invalid codepoint"))?,
        JsonView::Array(values) => JsonValue::parse(&format!(
            "[{}]",
            values
                .iter()
                .take(8)
                .map(JsonValue::to_json)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ))?,
        JsonView::Object(_) => return Err(Error::CommitKey),
        JsonView::Null | JsonView::Bool(_) | JsonView::Integer(_) | JsonView::Float(_) => {
            return Err(Error::CommitType)
        }
    };
    openpilot_runtime_version::python_str(&prefix)?
        .into_iter()
        .map(char::from_u32)
        .collect::<Option<String>>()
        .ok_or(Error::Contract(
            "commit filename contains a lone Unicode surrogate",
        ))
}
pub fn crash_filename(stamp: &str, commit: &JsonValue, path: &str) -> Result<String, Error> {
    Ok(format!(
        "{stamp}_{}_{}",
        commit_prefix(commit)?,
        safe_fn(&path.replace('/', "_"))
    )
    .chars()
    .take(MAX_TOMBSTONE_FN_LEN)
    .collect())
}
fn copy(source: &Path, destination: &Path) -> Result<PathBuf, Error> {
    let destination = if destination.is_dir() {
        destination.join(
            source
                .file_name()
                .ok_or(Error::Contract("missing crash filename"))?,
        )
    } else {
        destination.to_path_buf()
    };
    let original = fs::metadata(source)?;
    if let Ok(target) = fs::metadata(&destination) {
        if original.dev() == target.dev() && original.ino() == target.ino() {
            return Err(Error::SameFile);
        }
    }
    fs::copy(source, &destination)?;
    Ok(destination)
}
pub fn report_tombstone_apport<S: Sdk, I: Inputs, C: Clock>(
    filename: &Path,
    log_root: &Path,
    retrace: &Retrace,
    reporter: &mut Reporter<S, I>,
    clock: &mut C,
) -> Result<ReportOutcome, Error> {
    let size = fs::metadata(filename)?.len();
    if size > MAX_SIZE {
        reporter.logger.emit(
            log_site!(),
            path_record(
                Level::Error,
                "Tombstone ",
                filename,
                &format!(" too big, {size}. Skipping..."),
            )?,
        )?;
        return Ok(ReportOutcome::TooLarge);
    }
    let metadata = reader::metadata(filename)?;
    let trace = retrace.stacktrace(filename)?;
    let description = metadata.finish(&trace);
    reporter.report_tombstone_filename(
        &JsonValue::codepoints(path_points(filename))
            .ok_or(Error::Contract("invalid filename codepoint"))?,
        &description.message,
        &description.contents,
    )?;
    let stamp = clock.local_stamp();
    let build = reporter.inputs.build_metadata()?;
    let name = crash_filename(&stamp, &build.openpilot.git_commit, &description.path)?;
    let folder = log_root.join("crash");
    fs::create_dir_all(&folder)?;
    let destination = folder.join(name);
    let destination = copy(filename, &destination)?;
    match fs::remove_file(filename) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {}
        Err(error) => return Err(error.into()),
    }
    Ok(ReportOutcome::Copied(destination))
}
pub struct Daemon<S, I, C> {
    pub reporter: Reporter<S, I>,
    pub apport: PathBuf,
    pub log_root: PathBuf,
    pub retrace: Retrace,
    pub clock: C,
    should_report: bool,
    initial: HashSet<Tombstone>,
}
impl<S: Sdk, I: Inputs, C: Clock> Daemon<S, I, C> {
    pub fn start(
        mut reporter: Reporter<S, I>,
        apport: PathBuf,
        log_root: PathBuf,
        retrace: Retrace,
        clock: C,
    ) -> Result<Self, Error> {
        let should_report = reporter.init(Project::SelfdriveNative)?;
        clear_apport_folder(&apport);
        let initial = get_tombstones(&apport)?;
        Ok(Self {
            reporter,
            apport,
            log_root,
            retrace,
            clock,
            should_report,
            initial,
        })
    }
    pub fn should_report(&self) -> bool {
        self.should_report
    }
    /// Scan failure propagates; a per-file failure logs once and still advances the observed set.
    pub fn cycle(&mut self) -> Result<(), Error> {
        let now = get_tombstones(&self.apport)?;
        for entry in now.difference(&self.initial) {
            if !self.should_report {
                if let Err(_ignored_by_source) = fs::remove_file(&entry.path) {}
                continue;
            }
            let result = (|| -> Result<(), Error> {
                self.reporter.logger.emit(
                    log_site!(),
                    path_record(Level::Info, "reporting new tombstone ", &entry.path, "")?,
                )?;
                if entry
                    .path
                    .as_os_str()
                    .as_encoded_bytes()
                    .ends_with(b".crash")
                {
                    report_tombstone_apport(
                        &entry.path,
                        &self.log_root,
                        &self.retrace,
                        &mut self.reporter,
                        &mut self.clock,
                    )?;
                } else {
                    self.reporter.logger.emit(
                        log_site!(),
                        path_record(Level::Error, "unknown crash type: ", &entry.path, "")?,
                    )?;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.reporter.logger.emit(
                    log_site!(),
                    path_record(Level::Error, "Error reporting tombstone ", &entry.path, "")?
                        .with_exception(format!("{error}\n")),
                )?;
            }
        }
        self.initial = now;
        Ok(())
    }
}
