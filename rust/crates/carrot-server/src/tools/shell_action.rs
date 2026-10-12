use super::{
    context::{Context, Reply},
    policy,
    runner::{Command, Failure},
    shell, text,
};
use crate::Value;
use std::time::Duration;

pub(super) async fn run(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let command = text::strip(body.get("cmd"), "")?;
    if command.is_empty() {
        return Ok(context.invalid("missing cmd", ""));
    }
    let Some(mut argv) = shell::split(&command) else {
        return Ok(context.invalid("bad cmd format", ""));
    };
    if argv.is_empty() {
        return Ok(context.invalid("empty cmd", ""));
    }
    match argv[0].as_str() {
        "pull" | "status" | "branch" | "log" => {
            let subcommand = std::mem::replace(&mut argv[0], "git".into());
            argv.insert(1, subcommand);
        }
        _ => {}
    }
    if let Some(value) = policy::shell(&argv) {
        return Ok(Reply { status: 403, value });
    }
    context.progress(&command, 1, 1)?;
    context.append(&format!("$ {command}"))?;
    let result = if context.streaming() {
        context.command(&argv, 10, true).await?
    } else {
        context
            .runner
            .sync(Command {
                argv: &argv,
                cwd: Some(&context.config.paths.repository),
                timeout: Some(Duration::from_secs(10)),
            })
            .await?
    };
    let output = if let Some(id) = &context.id {
        text::string(
            context.jobs.get(id)?.unwrap_or(Value::Null).get("log"),
            true,
        )?
    } else if result.output.is_empty() {
        "(no output)".into()
    } else {
        result.output
    };
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(!context.streaming() || result.code == 0)),
        ("out", Value::text(&output)),
        ("returncode", Value::integer(result.code)),
    ])))
}
