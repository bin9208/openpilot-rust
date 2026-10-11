use super::text;
use crate::{
    tools::runner::{Command, Runner},
    Value,
};
use std::time::Duration;

const JOURNAL_TERMS: [&str; 12] = [
    "camerad",
    "camera",
    "encoderd",
    "loggerd",
    "webrtcd",
    "visionipc",
    "v4l_encoder",
    "spectra.cc",
    "h264",
    "multi-slice",
    "checked_ioctl",
    "carrot",
];

async fn run(runner: &Runner, argv: &[String], seconds: u64) -> Value {
    let arguments = Value::Array(argv.iter().map(|argument| Value::text(argument)).collect());
    match runner
        .raw_capture(Command {
            argv,
            cwd: None,
            timeout: Some(Duration::from_secs(seconds)),
        })
        .await
    {
        Ok(output) => {
            let decoded = |bytes: &[u8]| {
                String::from_utf8_lossy(bytes)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n")
            };
            Value::object([
                ("argv", arguments),
                (
                    "returncode",
                    Value::integer(crate::tools::runner::code(output.status)),
                ),
                (
                    "stdout",
                    Value::text(&text::trim(&decoded(&output.stdout), 96_000)),
                ),
                (
                    "stderr",
                    Value::text(&text::trim(&decoded(&output.stderr), 24_000)),
                ),
            ])
        }
        Err(error) => Value::object([
            ("argv", arguments),
            ("error", Value::text(&text::trim(&error.to_string(), 800))),
        ]),
    }
}
fn filtered(raw: Value, limit: usize, include: impl Fn(&str) -> bool) -> Value {
    let stdout = raw.get("stdout").string().unwrap_or_default();
    let rows: Vec<_> = text::lines(&stdout)
        .into_iter()
        .filter(|line| include(line))
        .collect();
    Value::object([
        ("command", raw.get("argv").clone()),
        ("returncode", raw.get("returncode").clone()),
        (
            "error",
            if raw.get("error").truth() {
                raw.get("error").clone()
            } else {
                Value::text("")
            },
        ),
        (
            "lines",
            Value::Array(
                rows.iter()
                    .skip(rows.len().saturating_sub(limit))
                    .map(|line| Value::text(&text::trim(line, 800)))
                    .collect(),
            ),
        ),
    ])
}
pub(super) async fn journal(runner: &Runner) -> Value {
    let argv = [
        "journalctl",
        "-b",
        "--no-pager",
        "-n",
        "900",
        "-o",
        "short-iso",
    ]
    .map(str::to_owned);
    filtered(run(runner, &argv, 4).await, 240, |line| {
        JOURNAL_TERMS
            .iter()
            .any(|term| line.to_lowercase().contains(term))
    })
}
pub(super) async fn sockets(runner: &Runner, processes: &Value) -> Value {
    let mut pids = Vec::new();
    if let Value::Object(groups) = processes {
        for (_, group) in groups {
            if let Value::Array(rows) = group {
                for row in rows {
                    if row.get("pid").truth() {
                        pids.push(format!(
                            "pid={},",
                            row.get("pid").string().unwrap_or_default()
                        ));
                    }
                }
            }
        }
    }
    filtered(
        run(runner, &["ss".into(), "-tunap".into()], 3).await,
        320,
        |line| line.contains(":5001") || pids.iter().any(|pid| line.contains(pid)),
    )
}
