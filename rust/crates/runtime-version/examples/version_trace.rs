//! Line protocol used only by the unchanged-source differential oracle.
use openpilot_runtime_version::{self as version, git, BuildMetadata, Error, JsonValue};
use serde::Deserialize;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Metadata {
        path: PathBuf,
    },
    FromJson {
        source: String,
    },
    Git {
        method: GitMethod,
        cwd: Option<PathBuf>,
        #[serde(default = "head")]
        revision: String,
    },
    Version {
        path: PathBuf,
    },
    ReleaseNotes {
        path: PathBuf,
    },
    Dirty {
        path: PathBuf,
    },
    Prebuilt {
        path: PathBuf,
    },
    Chdir {
        path: PathBuf,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum GitMethod {
    Commit,
    CommitDate,
    ShortBranch,
    Branch,
    Origin,
    NormalizedOrigin,
}
fn head() -> String {
    "HEAD".into()
}
fn error(error: Error) -> String {
    let kind = match error {
        Error::Io(ref error) => match error.kind() {
            io::ErrorKind::NotFound => "FileNotFoundError",
            io::ErrorKind::PermissionDenied => "PermissionError",
            io::ErrorKind::IsADirectory => "IsADirectoryError",
            io::ErrorKind::NotADirectory => "NotADirectoryError",
            _ => "OSError",
        },
        Error::Utf8(_) => "UnicodeDecodeError",
        Error::GitExit(_) => "CalledProcessError",
        Error::CachePoisoned | Error::Format(_) | Error::Logging(_) => "NativeError",
        Error::VersionHeader => "IndexError",
        Error::InvalidMetadata => "Exception",
        Error::Json(openpilot_logmessaged::JsonError::IntegerLimit) => "ValueError",
        Error::Json(_) => "JSONDecodeError",
        Error::Attribute(_) => "AttributeError",
        Error::Type(_) => "TypeError",
        Error::Key => "KeyError",
    };
    serde_json::json!({"error": kind}).to_string()
}
fn value(result: Result<JsonValue, Error>) -> Result<String, Error> {
    match result {
        Ok(value) => Ok(format!("{{\"value\":{}}}", value.to_json()?)),
        Err(failure) => Ok(error(failure)),
    }
}
fn boolean(value: bool) -> String {
    format!("{{\"value\":{value}}}")
}
fn metadata(build: BuildMetadata) -> Result<String, Error> {
    let fields = [
        ("version", &build.openpilot.version),
        ("release_notes", &build.openpilot.release_notes),
        ("git_commit", &build.openpilot.git_commit),
        ("git_origin", &build.openpilot.git_origin),
        ("git_commit_date", &build.openpilot.git_commit_date),
        ("build_style", &build.openpilot.build_style),
    ]
    .into_iter()
    .map(|(key, value)| Ok(format!("\"{key}\":{}", value.to_json()?)))
    .collect::<Result<Vec<_>, Error>>()?
    .join(",");
    let comma = match build.openpilot.comma_remote() {
        Ok(result) => boolean(result),
        Err(failure) => error(failure),
    };
    Ok(format!("{{\"value\":{{\"channel\":{},\"openpilot\":{{{fields},\"is_dirty\":{}}},\"properties\":{{\"short_version\":{},\"git_normalized_origin\":{},\"comma_remote\":{comma},\"tested_channel\":{},\"release_channel\":{},\"canonical\":{},\"ui_description\":{}}}}}}}",
        build.channel.to_json()?, build.openpilot.is_dirty,
        value(build.openpilot.short_version())?, value(build.openpilot.git_normalized_origin())?,
        boolean(build.tested_channel()), boolean(build.release_channel()), value(build.canonical())?, value(build.ui_description())?))
}
fn handle(request: Request) -> Result<String, Error> {
    match request {
        Request::Metadata { path } => metadata(version::get_build_metadata(&path)?),
        Request::FromJson { source } => metadata(version::build_metadata_from_dict(
            &JsonValue::parse(&source)?,
        )?),
        Request::Git {
            method,
            cwd,
            revision,
        } => value(
            match method {
                GitMethod::Commit => git::get_commit(cwd.as_deref(), &revision),
                GitMethod::CommitDate => git::get_commit_date(cwd.as_deref(), &revision),
                GitMethod::ShortBranch => match cwd.as_deref() {
                    Some(path) => git::get_short_branch(Some(path)),
                    None => git::get_short_branch_default(),
                },
                GitMethod::Branch => match cwd.as_deref() {
                    Some(path) => git::get_branch(Some(path)),
                    None => git::get_branch_default(),
                },
                GitMethod::Origin => match cwd.as_deref() {
                    Some(path) => git::get_origin(Some(path)),
                    None => git::get_origin_default(),
                },
                GitMethod::NormalizedOrigin => git::get_normalized_origin(cwd.as_deref()),
            }
            .map(|text| JsonValue::text(&text)),
        ),
        Request::Version { path } => {
            value(version::get_version(&path).map(|text| JsonValue::text(&text)))
        }
        Request::ReleaseNotes { path } => {
            value(version::get_release_notes(&path).map(|text| JsonValue::text(&text)))
        }
        Request::Dirty { path } => Ok(boolean(version::is_dirty(&path)?)),
        Request::Prebuilt { path } => Ok(boolean(version::is_prebuilt(&path)?)),
        Request::Chdir { path } => {
            std::env::set_current_dir(path)?;
            Ok("{\"value\":null}".into())
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let response = match handle(request) {
            Ok(response) => response,
            Err(failure) => error(failure),
        };
        writeln!(io::stdout(), "{response}")?;
        io::stdout().flush()?;
    }
    Ok(())
}
