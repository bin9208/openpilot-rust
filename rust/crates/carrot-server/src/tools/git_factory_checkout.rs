use super::{
    context::{Context, Reply},
    runner::Failure,
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn checkout(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let branch = text::string(body.get("branch"), true)?;
    if branch.is_empty() {
        return Ok(context.invalid("missing branch", ""));
    }
    let fetched = context.native_fetch().await?;
    if fetched.code != 0 {
        return context.result(fetched, None);
    }
    let local = branch.strip_prefix("origin/").unwrap_or(&branch);
    let result = context
        .stage_update(&format!("origin/{local}"), Some(local), false, false)
        .await?;
    let staged = result.code == 0;
    let mut reply = context.result(
        result,
        Some((
            "git_result_reset_repo_checkout_done",
            Value::object([("branch", Value::text(local))]),
        )),
    )?;
    set(&mut reply.value, "staged", Value::Bool(staged))?;
    set(&mut reply.value, "reboot_required", Value::Bool(staged))?;
    Ok(reply)
}
