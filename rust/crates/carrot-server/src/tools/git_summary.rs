use super::{context::Context, runner::Failure};
use crate::Value;
use num_traits::ToPrimitive;

pub(super) async fn build(
    context: &Context,
    before: &str,
    after: &str,
    raw: &str,
) -> Result<Value, Failure> {
    if before.is_empty() || after.is_empty() || before == after {
        return Ok(format(before, after, 0, Vec::new(), [0; 3], raw));
    }
    let range = format!("{before}..{after}");
    let count = context
        .bounded(
            &[
                "git".into(),
                "rev-list".into(),
                "--count".into(),
                range.clone(),
            ],
            15,
        )
        .await?;
    let count = if count.code == 0 {
        Value::text(&count.output)
            .int()
            .ok()
            .and_then(|value| value.to_usize())
            .unwrap_or(0)
    } else {
        0
    };
    let log = context
        .bounded(
            &[
                "git".into(),
                "log".into(),
                "--format=%h%x09%s".into(),
                "-20".into(),
                range,
            ],
            15,
        )
        .await?;
    let mut commits = Vec::new();
    if log.code == 0 {
        for line in log
            .output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let (hash, message) = line.split_once('\t').unwrap_or(("", line));
            commits.push(Value::object([
                ("hash", Value::text(hash.trim())),
                ("message", Value::text(message.trim())),
            ]));
        }
    }
    let stat = context
        .bounded(
            &[
                "git".into(),
                "diff".into(),
                "--shortstat".into(),
                before.into(),
                after.into(),
            ],
            20,
        )
        .await?;
    let counts = if stat.code == 0 {
        shortstat(&stat.output)
    } else {
        [0; 3]
    };
    Ok(format(
        before,
        after,
        if count == 0 { commits.len() } else { count },
        commits,
        counts,
        raw,
    ))
}
fn shortstat(output: &str) -> [usize; 3] {
    let mut values = [0; 3];
    let words: Vec<_> = output.split_whitespace().collect();
    for pair in words.windows(2) {
        let Ok(number) = pair[0].parse() else {
            continue;
        };
        if ["file", "files"].contains(&pair[1]) {
            values[0] = number;
        } else if pair[1].starts_with("insertion(") || pair[1].starts_with("insertions(") {
            values[1] = number;
        } else if pair[1].starts_with("deletion(") || pair[1].starts_with("deletions(") {
            values[2] = number;
        }
    }
    values
}
fn format(
    before: &str,
    after: &str,
    count: usize,
    commits: Vec<Value>,
    stat: [usize; 3],
    raw: &str,
) -> Value {
    if before.is_empty() || after.is_empty() || before == after || count == 0 {
        let display = "Already up to date";
        return Value::object([
            ("updated", Value::Bool(false)),
            ("before", Value::text(before)),
            ("after", Value::text(after)),
            ("commit_count", Value::integer(0)),
            ("commits", Value::Array(Vec::new())),
            ("files_changed", Value::integer(0)),
            ("insertions", Value::integer(0)),
            ("deletions", Value::integer(0)),
            ("display", Value::text(display)),
            ("card_summary", Value::text(display)),
            (
                "detail",
                Value::text(format!("{display}\n\nGit output\n{}", raw.trim()).trim()),
            ),
        ]);
    }
    let mut lines = vec!["New Updates".to_owned(), String::new()];
    for commit in commits.iter().take(3) {
        lines.push(commit.get("message").string().unwrap_or_default());
    }
    let shown = commits.len().min(3);
    if count > shown {
        lines.push(format!("and {} more commits", count - shown));
    }
    let mut bits = vec![format!(
        "{count} commit{}",
        if count == 1 { "" } else { "s" }
    )];
    if stat[0] > 0 {
        bits.push(format!(
            "{} file{}",
            stat[0],
            if stat[0] == 1 { "" } else { "s" }
        ));
    }
    bits.push(format!("+{} -{}", stat[1], stat[2]));
    lines.extend([String::new(), bits.join(" | ")]);
    let display = lines.join("\n").trim().to_owned();
    let mut detail = display.clone();
    let log = commits
        .iter()
        .map(|commit| {
            format!(
                "{} {}",
                commit.get("hash").string().unwrap_or_default(),
                commit.get("message").string().unwrap_or_default()
            )
            .trim()
            .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !log.is_empty() {
        detail.push_str(&format!("\n\nCommit log\n{log}"));
    }
    if !raw.trim().is_empty() {
        detail.push_str(&format!("\n\nGit output\n{}", raw.trim()));
    }
    Value::object([
        ("updated", Value::Bool(true)),
        ("before", Value::text(before)),
        ("after", Value::text(after)),
        ("commit_count", Value::integer(count)),
        ("commits", Value::Array(commits)),
        ("files_changed", Value::integer(stat[0])),
        ("insertions", Value::integer(stat[1])),
        ("deletions", Value::integer(stat[2])),
        ("display", Value::text(&display)),
        ("card_summary", Value::text(&display)),
        ("detail", Value::text(&detail)),
    ])
}
