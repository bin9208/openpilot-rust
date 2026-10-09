use super::{
    context::{Context, Reply},
    runner::{Completed, Failure},
    text,
};
use crate::{json_fields::set, Value};

pub(super) async fn change(context: &Context, body: &Value, add: bool) -> Result<Reply, Failure> {
    let name = if add {
        if context.streaming() {
            text::string(body.get("name"), true)?
        } else {
            text::strip(body.get("name"), "")?
        }
    } else {
        "origin".into()
    };
    let url = if add && !context.streaming() {
        text::strip(body.get("url"), "")?
    } else {
        text::string(body.get("url"), true)?
    };
    if name.is_empty() || url.is_empty() {
        return Ok(context.invalid(
            if add {
                "missing name or url"
            } else {
                "missing url"
            },
            "",
        ));
    }
    let exists = if add {
        let remotes = context.git(&["remote"], 15, false).await?;
        remotes.code == 0
            && remotes
                .output
                .split_whitespace()
                .any(|remote| remote == name)
    } else {
        true
    };
    let operation = if exists { "set-url" } else { "add" };
    context.progress(
        &format!(
            "{} {name} {url}",
            if add {
                format!("git remote {operation}")
            } else {
                operation.into()
            }
        ),
        1,
        2,
    )?;
    let setup = context
        .git(&["remote", operation, &name, &url], 30, true)
        .await?;
    if setup.code != 0 {
        return context.result(setup, None);
    }
    if context.streaming() {
        context.progress("checking Git configuration", 0, 2)?;
    }
    let repaired = context.repair(Some(name.clone()), false).await?;
    if context.streaming() && repaired.code != 0 {
        return context.result(repaired, None);
    }
    let summary = if add {
        "git_result_remote_add_done"
    } else {
        "git_result_remote_set_done"
    };
    let result = if add {
        let urls = context.git(&["remote", "-v"], 15, false).await?;
        if context.streaming() && urls.code == 0 && !urls.output.is_empty() {
            context.append("\n$ git remote -v\n")?;
            context.append(&(urls.output.clone() + "\n"))?;
        }
        Completed {
            streams: None,
            code: repaired.code,
            output: format!(
                "{}\n{}\n\n> git remote -v\n{}",
                setup.output,
                repaired.output,
                if urls.code == 0 {
                    urls.output.as_str()
                } else {
                    ""
                }
            )
            .trim()
            .into(),
        }
    } else {
        repaired
    };
    let mut reply = context.result(
        result,
        Some((
            summary,
            if add {
                Value::object([("name", Value::text(&name))])
            } else {
                Value::Null
            },
        )),
    )?;
    if !context.streaming() && add {
        let empty = !reply.value.get("out").truth();
        set(&mut reply.value, "empty_output", Value::Bool(empty))?;
    }
    Ok(reply)
}
