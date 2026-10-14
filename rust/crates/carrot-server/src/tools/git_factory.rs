use super::{
    context::{Context, Reply},
    runner::Failure,
};
use crate::Value;

const NATIVE_REMOTE: &str = "https://github.com/bin9208/openpilot-rust.git";
pub(super) async fn fetch(context: &Context) -> Result<Reply, Failure> {
    context.progress("configure native origin", 1, 2)?;
    let remote = context
        .git(&["remote", "set-url", "origin", NATIVE_REMOTE], 15, false)
        .await?;
    if remote.code != 0 {
        let added = context
            .git(&["remote", "add", "origin", NATIVE_REMOTE], 15, false)
            .await?;
        if added.code != 0 {
            return context.result(added, None);
        }
    }
    let configured = context
        .git(&["remote", "set-branches", "origin", "*"], 15, false)
        .await?;
    if configured.code != 0 {
        return context.result(configured, None);
    }
    let fetched = context.native_fetch().await?;
    if fetched.code != 0 {
        return context.result(fetched, None);
    }
    let listed = context
        .git(
            &[
                "for-each-ref",
                "--format=%(refname:strip=3)",
                "refs/remotes/origin/",
            ],
            15,
            false,
        )
        .await?;
    if listed.code != 0 {
        return context.result(listed, None);
    }
    let branches: std::collections::BTreeSet<_> = listed
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "HEAD")
        .collect();
    let output = format!(
        "Native origin configured; existing local branches and other remotes preserved.\n{}",
        fetched.output
    );
    context.append(&output)?;
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        (
            "summary_vars",
            Value::object([("count", Value::integer(branches.len()))]),
        ),
        (
            "branches",
            Value::Array(branches.iter().map(|branch| Value::text(branch)).collect()),
        ),
        ("out", Value::text(&output)),
        (
            "summary_key",
            Value::text("git_result_reset_repo_fetch_done"),
        ),
    ])))
}
