use super::{
    context::{Context, Reply},
    runner::{Completed, Failure},
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn list(context: &Context) -> Result<Reply, Failure> {
    context.progress("fetch --all --prune", 1, 2)?;
    let fetch = context
        .git(&["fetch", "--all", "--prune"], 180, true)
        .await?;
    if fetch.code != 0 {
        return context.result(fetch, None);
    }
    context.progress("git refs", 2, 2)?;
    let local = context
        .git(
            &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
            30,
            false,
        )
        .await?;
    let remote = context
        .git(
            &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
            30,
            false,
        )
        .await?;
    if context.streaming() && (!local.output.is_empty() || !remote.output.is_empty()) {
        context.append("\n$ git refs\n")?;
        if !local.output.is_empty() {
            context.append(&format!("[local]\n{}\n", local.output))?;
        }
        if !remote.output.is_empty() {
            context.append(&format!("[remote]\n{}\n", remote.output))?;
        }
    }
    if local.code != 0 || remote.code != 0 {
        let code = if local.code != 0 {
            local.code
        } else {
            remote.code
        };
        return context.result(
            Completed {
                streams: None,
                code,
                output: format!("{}\n\n{}\n{}", fetch.output, local.output, remote.output)
                    .trim()
                    .into(),
            },
            None,
        );
    }
    let current = context
        .git(&["branch", "--show-current"], 15, false)
        .await?;
    let remotes = context.git(&["remote"], 15, false).await?;
    let names: Vec<_> = if remotes.code == 0 {
        remotes
            .output
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    } else {
        vec!["origin".into()]
    };
    let urls = context.git(&["remote", "-v"], 15, false).await?;
    let mut remote_urls = Value::object([]);
    if urls.code == 0 {
        for line in urls.output.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() >= 2 && !remote_urls.has(fields[0]) {
                set(&mut remote_urls, fields[0], Value::text(fields[1]))?;
            }
        }
    }
    let mut items = Vec::new();
    for name in local
        .output
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if !items
            .iter()
            .any(|item: &Value| item.get("kind").text_eq("local") && item.get("name").text_eq(name))
        {
            items.push(Value::object([
                ("kind", Value::text("local")),
                ("ref", Value::text(name)),
                ("name", Value::text(name)),
                ("label", Value::text(name)),
            ]));
        }
    }
    let mut longest = names.clone();
    longest.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for reference in remote
        .output
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if let Some((name, branch)) = longest.iter().find_map(|name| {
            reference
                .strip_prefix(&format!("{name}/"))
                .map(|branch| (name, branch.trim()))
        }) {
            if branch.is_empty()
                || branch == "HEAD"
                || items.iter().any(|item| {
                    item.get("kind").text_eq("remote")
                        && item.get("remote").text_eq(name)
                        && item.get("name").text_eq(branch)
                })
            {
                continue;
            }
            items.push(Value::object([
                ("kind", Value::text("remote")),
                ("ref", Value::text(&format!("{name}/{branch}"))),
                ("remote", Value::text(name)),
                ("name", Value::text(branch)),
                ("label", Value::text(branch)),
            ]));
        }
    }
    items.sort_by_key(|item| {
        (
            if item.get("kind").text_eq("local") {
                0
            } else {
                1
            },
            text::string(item.get("remote"), true)
                .unwrap_or_default()
                .to_lowercase(),
            text::string(item.get("name"), true)
                .unwrap_or_default()
                .to_lowercase(),
        )
    });
    let branches = items
        .iter()
        .map(|item| item.get("ref").string().unwrap_or_default())
        .collect::<std::collections::BTreeSet<_>>();
    let device = crate::params_http::device_type();
    let fetch = if let Some(id) = &context.id {
        text::string(
            context.jobs.get(id)?.unwrap_or(Value::Null).get("log"),
            true,
        )?
    } else {
        fetch.output
    };
    Ok(Reply::ok(Value::object([
        ("ok", Value::Bool(true)),
        ("summary_key", Value::text("git_result_branch_list_done")),
        (
            "summary_vars",
            Value::object([("count", Value::integer(items.len()))]),
        ),
        (
            "branches",
            Value::Array(branches.iter().map(|name| Value::text(name)).collect()),
        ),
        ("branch_items", Value::Array(items)),
        (
            "current_branch",
            Value::text(if current.code == 0 {
                current.output.trim()
            } else {
                ""
            }),
        ),
        ("fetch", Value::text(&fetch)),
        ("device_type", Value::text(&device)),
        (
            "branch_prefix",
            Value::text(if device == "mici" { "c4" } else { "c3" }),
        ),
        (
            "remotes",
            Value::Array(names.iter().map(|name| Value::text(name)).collect()),
        ),
        ("remote_urls", remote_urls),
    ])))
}
