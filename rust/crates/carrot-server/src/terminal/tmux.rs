//! Original tmux subprocess and login bootstrap policy; no Python runtime bridge.
use super::Config;
use crate::{
    tools::{
        runner::{Command, Failure, Runner},
        shell::quote,
    },
    Error,
};
use std::{ffi::OsString, time::Duration};
use tokio::sync::watch;

fn current_user() -> Result<OsString, Error> {
    for name in ["USER", "USERNAME", "LOGNAME", "LNAME"] {
        if let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) {
            return Ok(value);
        }
    }
    let uid = rustix::process::getuid().as_raw().to_string();
    let passwd = std::fs::read_to_string("/etc/passwd")?;
    passwd
        .lines()
        .find_map(|line| {
            let fields: Vec<_> = line.split(':').collect();
            (fields.get(2) == Some(&uid.as_str()))
                .then(|| fields.first().copied())
                .flatten()
        })
        .map(Into::into)
        .ok_or_else(|| Error::Source("login username unavailable".into()))
}
pub(super) fn runner(config: &Config, stopped: watch::Receiver<bool>) -> Result<Runner, Error> {
    Ok(Runner {
        repository: std::env::current_dir()?,
        launcher: config.launcher.clone(),
        lock: None,
        stopped,
    })
}
pub(super) async fn run(
    runner: &Runner,
    argv: &[String],
    seconds: f64,
    check: bool,
) -> Result<(i32, String), Error> {
    let result = runner
        .sync(Command {
            argv,
            cwd: None,
            timeout: Some(Duration::from_secs_f64(seconds)),
        })
        .await;
    let completed = match result {
        Ok(completed) => completed,
        Err(Failure::Boundary(error)) => return Err(error),
        Err(Failure::Timeout) => {
            return Err(Error::Source(format!(
                "Command '{}' timed out after {} seconds",
                command_repr(argv),
                seconds
            )))
        }
        Err(Failure::Cancelled) => {
            return Err(Error::Source("terminal operation cancelled".into()))
        }
    };
    if check && completed.code != 0 {
        return Err(Error::Source(format!(
            "Command '{}' returned non-zero exit status {}.",
            command_repr(argv),
            completed.code
        )));
    }
    let (stdout, _) = completed
        .streams
        .ok_or_else(|| Error::Source("terminal command streams missing".into()))?;
    Ok((completed.code, stdout))
}
fn command_repr(argv: &[String]) -> String {
    format!(
        "[{}]",
        argv.iter()
            .map(|arg| format!("'{}'", arg.replace('\\', "\\\\").replace('\'', "\\'")))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
pub(super) async fn start_command(config: &Config, runner: &Runner) -> Result<String, Error> {
    let bootstrap = config.bootstrap();
    if current_user()? == "comma" {
        return Ok(bootstrap);
    }
    if rustix::process::geteuid().is_root() {
        return Ok(format!("exec su - comma -c {}", quote(&bootstrap)));
    }
    if executable("sudo") {
        let command = ["sudo", "-n", "-u", "comma", "true"].map(String::from);
        if run(runner, &command, 2., false)
            .await
            .is_ok_and(|(code, _)| code == 0)
        {
            return Ok(format!(
                "exec sudo -n -u comma -i bash -lc {}",
                quote(&bootstrap)
            ));
        }
    }
    Ok(bootstrap)
}
fn executable(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            let path = directory.join(name);
            path.is_file()
                && rustix::fs::accessat(
                    rustix::fs::CWD,
                    path,
                    rustix::fs::Access::EXEC_OK,
                    rustix::fs::AtFlags::empty(),
                )
                .is_ok()
        })
    })
}
pub(super) async fn ensure(config: &Config, runner: &Runner, session: &str) -> Result<bool, Error> {
    let (code, _) = run(
        runner,
        &[
            "tmux".into(),
            "has-session".into(),
            "-t".into(),
            session.into(),
        ],
        2.5,
        false,
    )
    .await?;
    let created = code != 0;
    if created {
        run(
            runner,
            &[
                "tmux".into(),
                "new-session".into(),
                "-d".into(),
                "-s".into(),
                session.into(),
                start_command(config, runner).await?,
            ],
            5.,
            true,
        )
        .await?;
    }
    for (name, value) in [("status", "off"), ("mouse", "on")] {
        run(
            runner,
            &[
                "tmux".into(),
                "set-option".into(),
                "-t".into(),
                session.into(),
                name.into(),
                value.into(),
            ],
            3.,
            false,
        )
        .await?;
    }
    Ok(created)
}
pub(super) async fn capture(
    config: &Config,
    runner: &Runner,
    session: &str,
) -> Result<String, Error> {
    let (code, output) = run(
        runner,
        &[
            "tmux".into(),
            "capture-pane".into(),
            "-p".into(),
            "-J".into(),
            "-t".into(),
            session.into(),
            "-S".into(),
            format!("-{}", config.capture_lines.max(40)),
        ],
        4.,
        false,
    )
    .await?;
    Ok(if code != 0 {
        String::new()
    } else if output.trim_end().is_empty() {
        " ".into()
    } else {
        output.trim_end().into()
    })
}
pub(super) async fn keys(
    runner: &Runner,
    session: &str,
    keys: &[String],
    literal: bool,
) -> Result<(), Error> {
    let mut argv = vec![
        "tmux".into(),
        "send-keys".into(),
        "-t".into(),
        session.into(),
    ];
    if literal {
        argv.push("-l".into());
    }
    argv.extend_from_slice(keys);
    run(runner, &argv, 4., true).await?;
    Ok(())
}
pub(super) async fn line(runner: &Runner, session: &str, line: &str) -> Result<(), Error> {
    if !line.is_empty() {
        keys(runner, session, &[line.into()], true).await?;
    }
    keys(runner, session, &["Enter".into()], false).await
}
