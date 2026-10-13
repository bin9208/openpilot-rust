use super::{
    context::{Context, Reply},
    runner::Failure,
};
use crate::Value;

pub(super) async fn run(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let count = if body.get("count").truth() {
        body.get("count").int().map_err(crate::Error::from)?
    } else {
        20.into()
    }
    .min(50.into());
    context.progress("git log", 1, 1)?;
    let log = context
        .git(&["log", "--oneline", &format!("-{count}")], 30, false)
        .await?;
    let head = context
        .git(&["rev-parse", "--short", "HEAD"], 10, false)
        .await?;
    if !log.output.is_empty() {
        context.append(&log.output)?;
    }
    let commits: Vec<_> = log
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (hash, message) = line.split_once(' ').unwrap_or((line, ""));
            Value::object([
                ("hash", Value::text(hash)),
                ("message", Value::text(message)),
            ])
        })
        .collect();
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(log.code == 0)),
        (
            "summary_vars",
            Value::object([("count", Value::integer(commits.len()))]),
        ),
        ("commits", Value::Array(commits)),
        (
            "current_commit",
            Value::text(if head.code == 0 {
                head.output.trim()
            } else {
                ""
            }),
        ),
        ("out", Value::text(&log.output)),
        ("summary_key", Value::text("git_result_log_done")),
    ])))
}
