use super::{
    context::{Context, Reply},
    git_summary,
    runner::{Completed, Failure},
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn run(context: &Context) -> Result<Reply, Failure> {
    let (prepared, target) = context.prepare_pull().await?;
    if !context.streaming() {
        context.clear_cache()?;
    }
    if prepared.code != 0 {
        return context.result(prepared, None);
    }
    if context.streaming() {
        context.progress("git reset --hard", 1, 2)?;
        context.append("$ git reset --hard\n")?;
        let reset = context.git(&["reset", "--hard"], 120, true).await?;
        if reset.code != 0 {
            return context.result(reset, None);
        }
    }
    let before = context.git(&["rev-parse", "HEAD"], 10, false).await?;
    let before = if before.code == 0 {
        before.output
    } else {
        String::new()
    };
    if context.streaming() {
        context.append("\n$ git pull\n")?;
        context.progress("git pull", 2, 2)?;
    }
    let pull = context
        .git(&["merge", "--ff-only", &target], 180, true)
        .await?;
    let after = context.git(&["rev-parse", "HEAD"], 10, false).await?;
    let after = if after.code == 0 {
        after.output
    } else {
        String::new()
    };
    context.clear_cache()?;
    let output = if let Some(id) = &context.id {
        text::string(
            context.jobs.get(id)?.unwrap_or(Value::Null).get("log"),
            true,
        )?
    } else {
        format!("{}\n{}", prepared.output, pull.output)
            .trim()
            .into()
    };
    if pull.code == 0 {
        if let (Some(runtime), Some(lock)) = (&context.config.auto_update, &context.runner.lock) {
            runtime
                .clear_for_tools(Arc::clone(lock), context.runner.stopped.clone())
                .await
                .map_err(|error| crate::Error::Source(error.to_string()))?;
        }
        context.pull_time(&output)?;
    }
    let summary = if pull.code == 0 {
        Some(git_summary::build(context, &before, &after, &output).await?)
    } else {
        None
    };
    let mut reply = context.result(
        Completed {
            streams: None,
            code: pull.code,
            output,
        },
        if !context.streaming() || summary.is_some() {
            Some(("git_result_pull_done", Value::Null))
        } else {
            None
        },
    )?;
    if let Some(summary) = summary {
        set(&mut reply.value, "update_summary", summary)?;
    }
    Ok(reply)
}
use std::sync::Arc;
