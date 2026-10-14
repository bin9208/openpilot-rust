mod command;
mod git_config;
mod process;
mod reboot;
mod repo_update;

use crate::{config::Config, terminal::Terminal, Error, Value};
use std::{ffi::OsString, fs, sync::Arc};

pub fn run(
    config: &Config,
    _terminal: &Arc<Terminal>,
    action: &str,
    payload: &Value,
) -> Result<Value, Error> {
    match action {
        "git_branches" => branches(config),
        "git_log" => log(config),
        "git_reboot" => reboot(config),
        "git_pull"
        | "git_sync"
        | "git_reset"
        | "git_checkout"
        | "git_checkout_commit"
        | "git_reset_repo"
        | "git_rebuild" => match command::git(config, action, payload)? {
            Some(command) => Ok(Value::object([
                ("ok", Value::Bool(true)),
                ("command", Value::text(&command)),
            ])),
            None => Ok(failure("invalid action or missing parameters")),
        },
        "rebuild_all" => Ok(Value::object([
            ("ok", Value::Bool(true)),
            (
                "command",
                Value::text(&format!(
                    "{} && sudo reboot",
                    command::git(config, "git_rebuild", payload)?
                        .ok_or_else(|| Error::Source("native repair command unavailable".into()))?
                )),
            ),
        ])),
        "send_tmux_log" | "server_tmux_log" => {
            if let Some(error) = capture_tmux(config)? {
                return Ok(failure(&error));
            }
            let mut result = if action == "server_tmux_log" {
                crate::upload::destinations(config, "tmux_send")?
            } else {
                Value::object([("ok", Value::Bool(true))])
            };
            match &mut result {
                Value::Object(fields) => fields.push((
                    "file".chars().map(u32::from).collect(),
                    Value::text("/download/tmux.log"),
                )),
                _ => return Err(Error::Source("tmux upload result must be an object".into())),
            }
            Ok(result)
        }
        _ => Ok(failure(&format!("unknown action: {action}"))),
    }
}

fn failure(error: &str) -> Value {
    Value::object([("ok", Value::Bool(false)), ("error", Value::text(error))])
}

fn branches(config: &Config) -> Result<Value, Error> {
    let fetched = match repo_update::capture(config, &["git", "fetch", "--all", "--prune"], 180) {
        Ok(result) => result,
        Err(repo_update::Failure::Busy(message)) => return Ok(failure(&format!("{message}\n"))),
        Err(repo_update::Failure::Runtime(error)) => return Err(error),
    };
    if fetched.code != 0 {
        return Ok(failure(&fetched.combined()));
    }
    let current = process::run(config, &["git", "branch", "--show-current"], 15, None)?.combined();
    let listed = process::run(
        config,
        &[
            "git",
            "for-each-ref",
            "--format=%(refname:short)\t%(refname)\t%(upstream:short)",
            "refs/heads/",
            "refs/remotes/",
        ],
        30,
        None,
    )?;
    if listed.code != 0 {
        return Ok(failure(&listed.combined()));
    }
    let mut branches = Vec::new();
    for line in git_config::text::lines(&listed.combined()) {
        let parts: Vec<_> = line.split('\t').collect();
        let [name, full, ..] = parts.as_slice() else {
            continue;
        };
        if name.ends_with("/HEAD") {
            continue;
        }
        let kind = if full.starts_with("refs/heads/") {
            "local"
        } else if full.starts_with("refs/remotes/") {
            "remote"
        } else {
            continue;
        };
        branches.push(Value::object([
            ("name", Value::text(name)),
            ("kind", Value::text(kind)),
            ("tracking", Value::text(parts.get(2).copied().unwrap_or(""))),
        ]));
    }
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        (
            "current",
            Value::text(current.trim_matches(git_config::text::whitespace)),
        ),
        ("branches", Value::Array(branches)),
    ]))
}

fn log(config: &Config) -> Result<Value, Error> {
    let listed = process::run(config, &["git", "log", "--oneline", "-30"], 30, None)?;
    if listed.code != 0 {
        return Ok(failure(&listed.combined()));
    }
    let head = process::run(config, &["git", "rev-parse", "--short", "HEAD"], 10, None)?;
    let current = if head.code == 0 {
        head.combined()
            .trim_matches(git_config::text::whitespace)
            .to_owned()
    } else {
        String::new()
    };
    let mut commits = Vec::new();
    for line in git_config::text::lines(&listed.combined()) {
        let line = line.trim_matches(git_config::text::whitespace);
        if line.is_empty() {
            continue;
        }
        let (hash, message) = line.split_once(' ').unwrap_or((line, ""));
        commits.push(Value::object([
            ("hash", Value::text(hash)),
            ("message", Value::text(message)),
        ]));
    }
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        ("commits", Value::Array(commits)),
        ("current", Value::text(&current)),
    ]))
}

fn reboot(config: &Config) -> Result<Value, Error> {
    for args in [&["sudo", "reboot"][..], &["reboot"][..]] {
        match reboot::spawn(config, args) {
            Ok(()) => {
                return Ok(Value::object([
                    ("ok", Value::Bool(true)),
                    ("rebooting", Value::Bool(true)),
                ]));
            }
            Err(Error::Process(openpilot_process_supervision::Error::Io(error)))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(Error::Process(error)) => {
                return Ok(failure(&process::launch_error(error, args[0])))
            }
            Err(error) => return Ok(failure(&error.to_string())),
        }
    }
    Ok(failure("reboot not available"))
}

fn capture_tmux(config: &Config) -> Result<Option<String>, Error> {
    match fs::remove_file(&config.tmux_log) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Ok(Some(process::io_error(&error, Some(&config.tmux_log)))),
    }
    let request = process::request(
        config,
        &["tmux", "capture-pane", "-pq", "-S-1000"],
        &std::env::current_dir()?,
    );
    let output = match process::capture(&request, 10, None) {
        Ok(output) => output,
        Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Some("tmux not available".into()));
        }
        Err(Error::Io(error)) => {
            return Ok(Some(process::io_error(
                &error,
                Some(std::path::Path::new("tmux")),
            )))
        }
        Err(error) => return Ok(Some(error.to_string())),
    };
    if output.code != 0 {
        let error = if !output.stderr.is_empty() {
            &output.stderr
        } else if !output.stdout.is_empty() {
            &output.stdout
        } else {
            "tmux capture failed"
        };
        let error = error.trim_matches(git_config::text::whitespace);
        return Ok(Some(if error.is_empty() {
            "tmux capture failed".into()
        } else {
            error.into()
        }));
    }
    if let Some(parent) = config.tmux_log.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&config.tmux_log, output.stdout)?;
    Ok(None)
}

pub fn internal_command(arguments: &[OsString]) -> Result<Option<i32>, Error> {
    repo_update::internal_command(arguments)
}

pub fn shutdown() -> Result<(), Error> {
    reboot::shutdown()
}
