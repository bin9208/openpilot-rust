use super::{
    context::{Context, Reply},
    runner::Failure,
    shell, text,
};
use crate::{json_fields::set, Value};

pub(super) async fn checkout(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let mut branch = text::strip(body.get("branch"), "")?;
    let kind = text::string(body.get("kind"), true)?;
    let name = text::string(body.get("name"), true)?;
    let remote = text::string(body.get("remote"), true)?;
    if branch.is_empty() && name.is_empty() {
        return Ok(context.invalid("missing branch", "MISSING_BRANCH"));
    }
    context.progress("fetch --all --prune", 1, 2)?;
    let fetch = context
        .git(&["fetch", "--all", "--prune"], 180, true)
        .await?;
    if fetch.code != 0 {
        return context.result(fetch, None);
    }
    context.progress(&format!("switch {branch}"), 2, 2)?;
    let remotes = context.git(&["remote"], 30, false).await?;
    let known: Vec<_> = if remotes.code == 0 {
        remotes.output.split_whitespace().collect()
    } else {
        vec!["origin"]
    };
    let tracked = if kind == "remote" {
        if remote.is_empty() || name.is_empty() {
            return Ok(context.invalid("missing remote branch info", ""));
        }
        if !known.contains(&remote.as_str()) {
            return Ok(context.invalid(&format!("unknown remote: {remote}"), ""));
        }
        branch = format!("{remote}/{name}");
        Some(name.clone())
    } else if kind == "local" {
        branch = if name.is_empty() { branch } else { name };
        None
    } else {
        known.iter().find_map(|remote| {
            branch
                .strip_prefix(&format!("{remote}/"))
                .map(str::to_owned)
        })
    };
    let summary = tracked.as_deref().unwrap_or(&branch).to_owned();
    let result = if context.streaming() {
        let script=match tracked {
            Some(local)=>format!("if git show-ref --verify --quiet {}; then git switch {}; else git switch -c {} --track {}; fi",shell::quote(&format!("refs/heads/{local}")),shell::quote(&local),shell::quote(&local),shell::quote(&branch)),
            None if kind=="local"=>format!("git switch {}",shell::quote(&branch)),
            None=>format!("git switch {} || git switch -c {} --track {}",shell::quote(&branch),shell::quote(&branch),shell::quote(&format!("origin/{branch}"))),
        };
        context
            .command(&["bash".into(), "-lc".into(), script], 180, true)
            .await?
    } else if let Some(local) = tracked {
        let checked = context
            .git(
                &[
                    "show-ref",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{local}"),
                ],
                180,
                false,
            )
            .await?;
        if checked.code == 0 {
            context.git(&["switch", &local], 180, false).await?
        } else {
            context
                .git(&["switch", "-c", &local, "--track", &branch], 180, false)
                .await?
        }
    } else {
        let result = context.git(&["switch", &branch], 180, false).await?;
        if result.code != 0 && kind != "local" {
            context
                .git(
                    &[
                        "switch",
                        "-c",
                        &branch,
                        "--track",
                        &format!("origin/{branch}"),
                    ],
                    180,
                    false,
                )
                .await?
        } else {
            result
        }
    };
    let mut reply = context.result(
        result,
        Some((
            "git_result_checkout_done",
            Value::object([("branch", Value::text(&summary))]),
        )),
    )?;
    if !context.streaming() {
        let empty = !reply.value.get("out").truth();
        set(&mut reply.value, "empty_output", Value::Bool(empty))?;
    }
    Ok(reply)
}
