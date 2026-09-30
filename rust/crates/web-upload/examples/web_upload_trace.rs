use openpilot_web_upload::{self as upload, Error, Fields, Value};
use serde::Deserialize;
use serde_json::{json, Value as Json};
use std::{
    cell::RefCell,
    io::{self, BufRead},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Command {
    Helpers {
        settings: Value,
        env: upload::Environment,
        metadata: Value,
        base: String,
        parts: Vec<String>,
        session_token: String,
        purpose: String,
    },
    Normalize {
        value: String,
        default: String,
    },
    Session {
        base: String,
        metadata: Value,
        purpose: String,
        sync: bool,
    },
    Health {
        base: String,
        token: String,
    },
    Complete {
        base: String,
        token: String,
        payload: Value,
    },
    Folder {
        folder: PathBuf,
        base: String,
        token: String,
        directory: String,
        remote_path: String,
        filenames: Option<Vec<String>>,
        cancel_after: Option<usize>,
        cancel_sent: Option<u64>,
        progress_failure: Option<usize>,
    },
    Tmux {
        url: String,
        headers: std::collections::BTreeMap<String, String>,
        payload: Value,
        tmux_path: PathBuf,
        settings_path: Option<PathBuf>,
    },
}
fn fields(value: Value) -> Result<Fields, Error> {
    match value {
        Value::Object(fields) => Ok(fields),
        _ => Err(Error::Source("fixture requires object".into())),
    }
}
fn json_fields(value: &Fields) -> Result<Json, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&value.to_json()?)?)
}
fn error(error: Error) -> Json {
    let kind = match &error {
        Error::InvalidBaseUrl | Error::MissingBaseUrl | Error::Url(_) => "ValueError",
        Error::BodyShape(_) => "AttributeError",
        Error::Io(_) => "OSError",
        Error::MissingSession
        | Error::MissingToken
        | Error::Canceled
        | Error::InvalidFilename
        | Error::FilenameEncoding
        | Error::MissingFile(_)
        | Error::Folder(_)
        | Error::Source(_)
        | Error::File { .. }
        | Error::Format(_) => "RuntimeError",
        Error::Http(_) | Error::Request(_) => "TransportError",
        Error::Decode => "UnicodeDecodeError",
    };
    json!({"error":error.to_string(),"error_type":kind})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let command: Command = serde_json::from_str(&line?)?;
        let mut progress = Vec::new();
        let mut checks = 0;
        let result = (|| -> Result<Json, Error> {
            Ok(match command {
                Command::Normalize { value, default } => {
                    json!(upload::normalize_base_url(&value, &default)?)
                }
                Command::Helpers {
                    settings,
                    env,
                    metadata,
                    base,
                    parts,
                    session_token,
                    purpose,
                } => {
                    let settings = fields(settings)?;
                    let metadata = fields(metadata)?;
                    let payload = upload::session_payload(&metadata, &purpose)?;
                    json!({"settings":upload::web_settings(&settings,&env)?,"api_url":upload::api_url(&base,&parts.iter().map(String::as_str).collect::<Vec<_>>())?,"tmux":upload::tmux_target(&settings,&env,&session_token)?,"carrot_logs":upload::carrot_logs_target(&env)?,"device_id":upload::device_id(&metadata)?,"session_payload":json_fields(&payload).map_err(|error| Error::Source(error.to_string()))?})
                }
                Command::Session {
                    base,
                    metadata,
                    purpose,
                    sync,
                } => json!(upload::create_session_with_purpose(
                    &base,
                    &fields(metadata)?,
                    &purpose,
                    if sync {
                        upload::SessionMode::Sync
                    } else {
                        upload::SessionMode::Async
                    }
                )?),
                Command::Health { base, token } => upload::health(&base, &token),
                Command::Complete {
                    base,
                    token,
                    payload,
                } => upload::send_complete(&base, &token, &fields(payload)?),
                Command::Folder {
                    folder,
                    base,
                    token,
                    directory,
                    remote_path,
                    filenames,
                    cancel_after,
                    cancel_sent,
                    progress_failure,
                } => {
                    let sent = RefCell::new(0_u64);
                    let mut cancel = || {
                        checks += 1;
                        cancel_after.is_some_and(|limit| checks >= limit)
                            || cancel_sent.is_some_and(|limit| *sent.borrow() >= limit)
                    };
                    let mut callback = |value: upload::Progress<'_>| {
                        *sent.borrow_mut() = value.sent;
                        progress.push(json!([value.filename, value.sent, value.size, value.chunk]));
                        if progress_failure == Some(progress.len()) {
                            Err(Error::Source("progress callback failed".into()))
                        } else {
                            Ok(())
                        }
                    };
                    let mut observer = upload::Observer {
                        cancel: Some(&mut cancel),
                        progress: Some(&mut callback),
                    };
                    json!(upload::FolderUpload {
                        local_folder: &folder,
                        directory: &directory,
                        remote_path: &remote_path,
                        base_url: &base,
                        token: &token,
                        filenames: filenames.as_deref()
                    }
                    .run(&mut observer)?)
                }
                Command::Tmux {
                    url,
                    headers,
                    payload,
                    tmux_path,
                    settings_path,
                } => {
                    let target = upload::Target { url, headers };
                    let payload = fields(payload)?;
                    let response = upload::TmuxUpload {
                        target: &target,
                        payload: &payload,
                        tmux_path: &tmux_path,
                        settings_path: settings_path.as_deref().map(Path::as_ref),
                    }
                    .post()?;
                    json!({"status":response.status,"body":String::from_utf8_lossy(&response.body)})
                }
            })
        })();
        let mut output = match result {
            Ok(value) => json!({"result":value}),
            Err(value) => error(value),
        };
        output["progress"] = json!(progress);
        output["cancel_checks"] = json!(checks);
        println!("{}", output);
    }
    Ok(())
}
