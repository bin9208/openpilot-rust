use super::{
    context::{Context, Reply},
    policy::Action,
    runner::{Command, Failure},
};
use crate::{Error, Value};
use std::{fs, path::Path, process::Command as Process};

fn detached(args: &[String]) -> Result<(), Failure> {
    let mut child = Process::new(&args[0])
        .args(&args[1..])
        .spawn()
        .map_err(|error| crate::state::io_error(error, Path::new(&args[0])))?;
    std::thread::spawn(move || {
        if let Err(error) = child.wait() {
            eprintln!("Tools detached child: {error}");
        }
    });
    Ok(())
}
pub(super) async fn run(context: &Context, action: Action) -> Result<Reply, Failure> {
    match action {
        Action::CaptureTmux => {
            context.progress("capture tmux", 1, 1)?;
            let path = &context.config.paths.tmux_log;
            let target = path.clone();
            let removed = tokio::task::spawn_blocking(move || fs::remove_file(target))
                .await
                .map_err(|error| Error::Source(error.to_string()))?;
            match removed {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Ok(capture_failure(
                        crate::state::io_error(error, path).to_string(),
                    ))
                }
            }
            let argv = [
                "tmux".into(),
                "capture-pane".into(),
                "-pq".into(),
                "-S-1000".into(),
            ];
            let result = context
                .runner
                .sync(Command {
                    argv: &argv,
                    cwd: None,
                    timeout: None,
                })
                .await?;
            let (stdout, stderr) = result.streams.unwrap_or_default();
            if result.code != 0 {
                return Ok(capture_failure(if stderr.is_empty() {
                    stdout.trim().into()
                } else {
                    stderr.trim().into()
                }));
            }
            let target = path.clone();
            tokio::task::spawn_blocking(move || -> Result<(), Error> {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| crate::state::io_error(error, parent))?;
                }
                fs::write(&target, &stdout)
                    .map_err(|error| crate::state::io_error(error, &target))?;
                Ok(())
            })
            .await
            .map_err(|error| Error::Source(error.to_string()))??;
            Ok(Reply::ok(Value::object([
                ("ok", Value::Bool(true)),
                ("out", Value::text("tmux log captured")),
                ("file", Value::text("/download/tmux.log")),
            ])))
        }
        Action::Calibration => {
            let result = super::files::calibration(context)?;
            if context.streaming() {
                if let Some(id) = &context.id {
                    context.jobs.finish(id, true, result.value.clone())?;
                }
                let mut stopped = context.runner.stopped.clone();
                tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(1))=>{},_=stopped.changed()=>return Err(Failure::Cancelled)}
                detached(&["sudo".into(), "reboot".into()])?;
            } else {
                detached(&["bash".into(), "-lc".into(), "sleep 1 && sudo reboot".into()])?;
            }
            Ok(result)
        }
        Action::Reboot => {
            context.progress("request reboot", 1, 1)?;
            detached(&["sudo".into(), "reboot".into()])?;
            Ok(Reply::ok(Value::object([
                ("ok", Value::Bool(true)),
                ("out", Value::text("reboot requested")),
            ])))
        }
        Action::Rebuild => {
            context.progress("rebuild all", 1, 1)?;
            let script = format!(
                "cd {} && scons -c && rm -rf prebuilt && sudo reboot",
                super::shell::quote(&context.config.paths.repository.to_string_lossy())
            );
            detached(&["bash".into(), "-lc".into(), script])?;
            Ok(Reply::ok(Value::object([
                ("ok", Value::Bool(true)),
                (
                    "out",
                    Value::text("rebuild_all requested (clean + remove prebuilt + reboot)"),
                ),
            ])))
        }
        _ => Err(Error::Source("unexpected Tools system action".into()).into()),
    }
}
fn capture_failure(output: String) -> Reply {
    Reply::ok(Value::object([
        ("ok", Value::Bool(false)),
        ("error", Value::text("tmux capture failed")),
        ("error_code", Value::text("TMUX_CAPTURE_FAIL")),
        ("out", Value::text(&output)),
    ]))
}
