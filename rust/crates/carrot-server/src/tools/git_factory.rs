use super::{
    context::{Context, Reply},
    runner::{Completed, Failure},
    text,
};
use crate::{json_fields::set, Value};

const ORIGINAL_REMOTE: &str = "https://github.com/ajouatom/openpilot.git";
pub(super) async fn fetch(context: &Context) -> Result<Reply, Failure> {
    let mut output = String::new();
    context
        .command(
            &[
                "find".into(),
                ".git".into(),
                "-type".into(),
                "f".into(),
                "-name".into(),
                "*.lock".into(),
                "-delete".into(),
            ],
            10,
            false,
        )
        .await?;
    context.progress("configuring origin remote", 1, 4)?;
    let remote = context
        .git(&["remote", "set-url", "origin", ORIGINAL_REMOTE], 15, false)
        .await?;
    if remote.code != 0 {
        context.append("origin not found, adding new remote\n")?;
        context
            .git(&["remote", "remove", "origin"], 10, false)
            .await?;
        let added = context
            .git(&["remote", "add", "origin", ORIGINAL_REMOTE], 15, false)
            .await?;
        output.push_str(&format!(
            "> git remote add origin {ORIGINAL_REMOTE}\n{}\n\n",
            added.output
        ));
        if added.code != 0 {
            context.append(&format!("failed to add origin: {}\n", added.output))?;
            return Ok(Reply::ok(Value::object([
                ("ok", Value::Bool(false)),
                (
                    "error",
                    Value::text(&format!("failed to configure remote: {}", added.output)),
                ),
            ])));
        }
    } else {
        output.push_str(&format!(
            "> git remote set-url origin {ORIGINAL_REMOTE}\n{}\n\n",
            remote.output
        ));
    }
    context.append(&format!("origin → {ORIGINAL_REMOTE}\n"))?;
    let branches = context
        .git(&["remote", "set-branches", "origin", "*"], 15, false)
        .await?;
    output.push_str(&format!(
        "> git remote set-branches origin '*'\n{}\n\n",
        branches.output
    ));
    if branches.code != 0 {
        if context.streaming() {
            context.append(&format!(
                "failed to configure origin branches: {}\n",
                branches.output
            ))?;
            return Ok(Reply::ok(Value::object([
                ("ok", Value::Bool(false)),
                (
                    "error",
                    Value::text(&format!(
                        "failed to configure origin branches: {}",
                        branches.output
                    )),
                ),
            ])));
        }
        return context.result(
            Completed {
                code: branches.code,
                output: output.trim().into(),
                streams: None,
            },
            None,
        );
    }
    context.append("origin branches: *\n")?;
    context.progress("cleaning other remotes", 2, 4)?;
    let remotes = context.git(&["remote"], 10, false).await?;
    for remote in remotes
        .output
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty() && *remote != "origin")
    {
        context.append(&format!("removing remote: {remote}\n"))?;
        context
            .git(&["remote", "remove", remote], 10, false)
            .await?;
        output.push_str(&format!("> removed remote: {remote}\n"));
    }
    context.progress("git fetch origin --prune --force", 3, 4)?;
    context.git(&["pack-refs", "--all"], 20, false).await?;
    context
        .command(
            &["rm".into(), "-rf".into(), ".git/refs/remotes/origin".into()],
            10,
            false,
        )
        .await?;
    let fetched = context
        .git(&["fetch", "origin", "--prune", "--force"], 300, true)
        .await?;
    output.push_str(&format!(
        "> git fetch origin --prune --force\n{}\n\n",
        fetched.output
    ));
    if fetched.code != 0 {
        return context.result(
            Completed {
                code: fetched.code,
                output: output.trim().into(),
                streams: None,
            },
            None,
        );
    }
    context.progress("listing branches", 4, 4)?;
    let branches = context.git(&["branch", "-r"], 15, false).await?;
    let branches: std::collections::BTreeSet<_> = branches
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains("->"))
        .filter_map(|line| line.strip_prefix("origin/").map(str::to_owned))
        .collect();
    context.append(&format!("found {} branches\n", branches.len()))?;
    let out = if let Some(id) = &context.id {
        text::string(
            context.jobs.get(id)?.unwrap_or(Value::Null).get("log"),
            true,
        )?
    } else {
        output.trim().into()
    };
    let mut value = Value::object([
        ("ok", Value::Bool(true)),
        (
            "summary_vars",
            Value::object([("count", Value::integer(branches.len()))]),
        ),
        (
            "branches",
            Value::Array(branches.iter().map(|branch| Value::text(branch)).collect()),
        ),
        ("out", Value::text(&out)),
        (
            "summary_key",
            Value::text("git_result_reset_repo_fetch_done"),
        ),
    ]);
    if !context.streaming() {
        set(&mut value, "empty_output", Value::Bool(out.is_empty()))?;
    }
    Ok(Reply::ok(value))
}
