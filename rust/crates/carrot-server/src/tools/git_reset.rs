use super::{
    context::{Context, Reply},
    runner::{Completed, Failure},
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn reset(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let mode = text::strip(body.get("mode"), "hard")?;
    let target = text::strip(body.get("target"), "HEAD")?;
    if !["hard", "soft", "mixed"].contains(&mode.as_str()) {
        return Ok(context.invalid("bad mode", "INVALID_RESET_MODE"));
    }
    if context.streaming() {
        context.progress("checking Git configuration", 0, 2)?;
    }
    let repair = context.repair(None, true).await?;
    if repair.code != 0 {
        return context.result(repair, None);
    }
    context.progress(&format!("git reset --{mode} {target}"), 1, 1)?;
    let result = context
        .git(&["reset", &format!("--{mode}"), &target], 120, true)
        .await?;
    context.clear_cache()?;
    let result = Completed {
        streams: None,
        code: result.code,
        output: format!("{}\n{}", repair.output, result.output)
            .trim()
            .into(),
    };
    let mut reply = context.result(
        result,
        Some((
            "git_result_reset_done",
            Value::object([
                ("mode", Value::text(&mode)),
                ("target", Value::text(&target)),
            ]),
        )),
    )?;
    if !context.streaming() {
        let empty = !reply.value.get("out").truth();
        set(&mut reply.value, "empty_output", Value::Bool(empty))?;
    }
    Ok(reply)
}
pub(super) async fn sync(context: &Context) -> Result<Reply, Failure> {
    context.progress("delete local branches", 1, 2)?;
    let branches = context
        .command(
            &[
                "bash".into(),
                "-lc".into(),
                "git branch | grep -v '^\\*' | xargs -r git branch -D".into(),
            ],
            120,
            true,
        )
        .await?;
    if branches.code != 0 {
        return context.result(branches, None);
    }
    context.progress("fetch --all --prune", 2, 2)?;
    let fetch = context
        .git(&["fetch", "--all", "--prune"], 180, true)
        .await?;
    let mut reply = context.result(
        Completed {
            streams: None,
            code: fetch.code,
            output: format!("{}\n\n{}", branches.output, fetch.output)
                .trim()
                .into(),
        },
        Some(("git_result_sync_done", Value::Null)),
    )?;
    if !context.streaming() {
        let empty = !reply.value.get("out").truth();
        set(&mut reply.value, "empty_output", Value::Bool(empty))?;
    }
    Ok(reply)
}
