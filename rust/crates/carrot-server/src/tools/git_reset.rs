use super::{
    context::{Context, Reply},
    runner::Failure,
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn reset(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let mode = text::strip(body.get("mode"), "hard")?;
    let target = text::strip(body.get("target"), "HEAD")?;
    if !["hard", "soft", "mixed"].contains(&mode.as_str()) {
        return Ok(context.invalid("bad mode", "INVALID_RESET_MODE"));
    }
    if mode != "hard" {
        return Ok(context.invalid(
            "Soft and mixed reset cannot preserve a verified native source/package pair",
            "NATIVE_RESET_MODE",
        ));
    }
    let result = context.stage_update(&target, None, false, false).await?;
    let staged = result.code == 0;
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
    set(&mut reply.value, "staged", Value::Bool(staged))?;
    set(&mut reply.value, "reboot_required", Value::Bool(staged))?;
    Ok(reply)
}
pub(super) async fn sync(context: &Context) -> Result<Reply, Failure> {
    // Updating remote refs must not delete local user branches or installed bins.
    let fetched = context.native_fetch().await?;
    context.result(fetched, Some(("git_result_sync_done", Value::Null)))
}
