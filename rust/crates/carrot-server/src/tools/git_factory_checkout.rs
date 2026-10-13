use super::{
    context::{Context, Reply},
    runner::{Completed, Failure},
    text,
};
use crate::Value;

pub(super) async fn checkout(context: &Context, body: &Value) -> Result<Reply, Failure> {
    let branch = text::string(body.get("branch"), true)?;
    if branch.is_empty() {
        return Ok(context.invalid("missing branch", ""));
    }
    let steps: Vec<(Vec<String>, bool)> = vec![
        (vec!["git".into(), "merge".into(), "--abort".into()], true),
        (vec!["git".into(), "rebase".into(), "--abort".into()], true),
        (
            vec!["git".into(), "cherry-pick".into(), "--abort".into()],
            true,
        ),
        (vec!["git".into(), "revert".into(), "--abort".into()], true),
        (vec!["git".into(), "am".into(), "--abort".into()], true),
        (vec!["git".into(), "bisect".into(), "reset".into()], true),
        (
            vec![
                "git".into(),
                "checkout".into(),
                "-f".into(),
                "-B".into(),
                branch.clone(),
                format!("origin/{branch}"),
            ],
            false,
        ),
        (
            vec![
                "git".into(),
                "reset".into(),
                "--hard".into(),
                format!("origin/{branch}"),
            ],
            false,
        ),
        (vec!["git".into(), "clean".into(), "-xfd".into()], false),
    ];
    let mut output = String::new();
    for (index, (argv, allow_fail)) in steps.iter().enumerate() {
        context.progress(&argv.join(" "), i64::try_from(index + 1).unwrap_or(0), 9)?;
        let result = context.command(argv, 120, true).await?;
        output.push_str(&format!("> {}\n{}\n\n", argv.join(" "), result.output));
        if result.code != 0 && !*allow_fail {
            return context.result(
                Completed {
                    code: result.code,
                    output: output.trim().into(),
                    streams: None,
                },
                None,
            );
        }
    }
    if context.streaming() {
        context.result(
            Completed {
                code: 0,
                output,
                streams: None,
            },
            Some((
                "git_result_reset_repo_checkout_done",
                Value::object([("branch", Value::text(&branch))]),
            )),
        )
    } else {
        let out = output.trim();
        Ok(Reply::ok(Value::object([
            ("ok", Value::Bool(true)),
            ("out", Value::text(out)),
            (
                "summary_key",
                Value::text("git_result_reset_repo_checkout_done"),
            ),
            (
                "summary_vars",
                Value::object([("branch", Value::text(&branch))]),
            ),
            ("empty_output", Value::Bool(out.is_empty())),
        ])))
    }
}
