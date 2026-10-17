use super::{
    context::{Context, Reply},
    runner::{Completed, Failure},
};
use crate::{json_fields::set, Value};
use std::sync::Arc;

pub(super) async fn run(context: &Context) -> Result<Reply, Failure> {
    let (prepared, target) = context.prepare_pull().await?;
    if prepared.code != 0 {
        return context.result(prepared, None);
    }
    let before = context.git(&["rev-parse", "HEAD"], 15, false).await?;
    if before.code != 0 {
        return context.result(before, None);
    }
    let staged = context
        .stage_update(target.trim(), None, false, true)
        .await?;
    let ok = staged.code == 0;
    let output = format!("{}\n{}", prepared.output, staged.output)
        .trim()
        .to_owned();
    if ok {
        if let (Some(runtime), Some(lock)) = (&context.config.auto_update, &context.runner.lock) {
            runtime
                .clear_for_tools(Arc::clone(lock), context.runner.stopped.clone())
                .await
                .map_err(|error| crate::Error::Source(error.to_string()))?;
        }
    }
    let summary = if ok {
        context.record_staged_update()?;
        Some(
            super::git_summary::build(context, before.output.trim(), target.trim(), &output)
                .await?,
        )
    } else {
        None
    };
    context.clear_cache()?;
    let mut reply = context.result(
        Completed {
            code: staged.code,
            output,
            streams: None,
        },
        Some(("git_result_pull_done", Value::Null)),
    )?;
    if let Some(summary) = summary {
        set(&mut reply.value, "update_summary", summary)?;
    }
    set(&mut reply.value, "staged", Value::Bool(ok))?;
    set(
        &mut reply.value,
        "staged_commit",
        Value::text(target.trim()),
    )?;
    set(&mut reply.value, "reboot_required", Value::Bool(ok))?;
    Ok(reply)
}
