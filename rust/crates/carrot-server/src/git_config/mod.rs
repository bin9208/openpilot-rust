//! Source: openpilot/selfdrive/carrot/server/services/git_config.py (#225).
mod commands;
mod repair;
mod text;
mod upstream;

use crate::Error;
use std::{os::fd::BorrowedFd, path::Path};

/// The caller holds the transaction lock around this multi-command operation.
pub struct Repository<'a> {
    pub directory: &'a Path,
    pub launcher: &'a Path,
    pub lock: Option<BorrowedFd<'a>>,
}

enum Failure {
    Caught(String),
    Boundary(Error),
}

pub fn repair_git_config(
    repository: &Repository<'_>,
    remote: Option<&str>,
    repair_upstream: bool,
) -> Result<(i32, String), Error> {
    let mut messages = Vec::new();
    let options = repair::RepairOptions {
        remote,
        upstream: repair_upstream,
    };
    match repair::run(repository, options, &mut messages) {
        Ok(Some(message)) => Ok((0, message)),
        Ok(None) => Ok((0, messages.join("\n"))),
        Err(Failure::Caught(message)) => {
            messages.push(format!("Git configuration repair failed: {message}"));
            Ok((1, messages.join("\n")))
        }
        Err(Failure::Boundary(error)) => Err(error),
    }
}

pub fn prepare_git_pull(repository: &Repository<'_>) -> Result<(i32, String, String), Error> {
    let (code, output) = repair_git_config(repository, None, true)?;
    if code != 0 {
        return Ok((code, output, String::new()));
    }
    match commands::run(
        repository,
        &["rev-parse", "--verify", "@{upstream}^{commit}"],
        commands::Deadline::Integer(15),
    ) {
        Ok(result) if result.code != 0 => Ok((
            result.code,
            format!("{output}\n{}", result.stderr),
            String::new(),
        )),
        Ok(result) => Ok((0, output, result.stdout)),
        Err(Failure::Caught(message)) => Ok((1, format!("{output}\n{message}"), String::new())),
        Err(Failure::Boundary(error)) => Err(error),
    }
}
