use super::{
    context::{Context, Reply},
    runner::Failure,
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn checkout(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let branch = text::strip(body.get("branch"), "")?;
    let kind = text::string(body.get("kind"), true)?;
    let name = text::string(body.get("name"), true)?;
    let remote = text::string(body.get("remote"), true)?;
    if branch.is_empty() && name.is_empty() {
        return Ok(context.invalid("missing branch", "MISSING_BRANCH"));
    }
    if kind == "remote" && (remote != "origin" || name.is_empty()) {
        return Ok(context.invalid(
            "Native runtime branches must use origin from bin9208/openpilot-rust",
            "INVALID_NATIVE_ORIGIN",
        ));
    }
    let fetched = context.native_fetch().await?;
    if fetched.code != 0 {
        return context.result(fetched, None);
    }
    let selected = if name.is_empty() { branch } else { name };
    let local = selected.strip_prefix("origin/").unwrap_or(&selected);
    let target = if kind == "remote" || selected.starts_with("origin/") {
        format!("origin/{local}")
    } else {
        let exists = context
            .git(
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{local}"),
                ],
                15,
                false,
            )
            .await?;
        if exists.code == 0 {
            local.into()
        } else {
            format!("origin/{local}")
        }
    };
    let result = context
        .stage_update(&target, Some(local), false, false)
        .await?;
    let staged = result.code == 0;
    let mut reply = context.result(
        result,
        Some((
            "git_result_checkout_done",
            Value::object([("branch", Value::text(local))]),
        )),
    )?;
    set(&mut reply.value, "staged", Value::Bool(staged))?;
    set(&mut reply.value, "reboot_required", Value::Bool(staged))?;
    Ok(reply)
}
