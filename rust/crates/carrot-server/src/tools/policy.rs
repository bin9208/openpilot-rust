//! Exact nineteen registered action names and read-only shell policy from tools/actions.py.
use crate::{Error, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Backup,
    DeleteLogs,
    DeleteVideos,
    Branches,
    Checkout,
    Log,
    Pull,
    RemoteAdd,
    RemoteSet,
    Reset,
    FactoryCheckout,
    FactoryFetch,
    Sync,
    Reboot,
    Rebuild,
    Calibration,
    CaptureTmux,
    SendTmux,
    Shell,
}
impl Action {
    pub fn parse(value: &Value) -> Result<Self, Value> {
        let normalized =
            crate::param_changes::text::stripped(value, true).unwrap_or_else(|_| Value::text(""));
        match normalized.string().unwrap_or_default().as_str() {
            "backup_settings" => Ok(Self::Backup),
            "delete_all_logs" => Ok(Self::DeleteLogs),
            "delete_all_videos" => Ok(Self::DeleteVideos),
            "git_branch_list" => Ok(Self::Branches),
            "git_checkout" => Ok(Self::Checkout),
            "git_log" => Ok(Self::Log),
            "git_pull" => Ok(Self::Pull),
            "git_remote_add" => Ok(Self::RemoteAdd),
            "git_remote_set" => Ok(Self::RemoteSet),
            "git_reset" => Ok(Self::Reset),
            "git_reset_repo_checkout" => Ok(Self::FactoryCheckout),
            "git_reset_repo_fetch" => Ok(Self::FactoryFetch),
            "git_sync" => Ok(Self::Sync),
            "reboot" => Ok(Self::Reboot),
            "rebuild_all" => Ok(Self::Rebuild),
            "reset_calib" => Ok(Self::Calibration),
            "send_tmux_log" => Ok(Self::CaptureTmux),
            "server_tmux_log" => Ok(Self::SendTmux),
            "shell_cmd" => Ok(Self::Shell),
            _ => Err(Value::object([
                ("ok", Value::Bool(false)),
                (
                    "error",
                    if !normalized.truth() {
                        Value::text("missing action")
                    } else {
                        let mut points: Vec<u32> =
                            "unknown action: ".chars().map(u32::from).collect();
                        if let Value::Text(action) = &normalized {
                            points.extend(action);
                        }
                        Value::Text(points)
                    },
                ),
                (
                    "error_code",
                    Value::text(if !normalized.truth() {
                        "MISSING_TOOL_ACTION"
                    } else {
                        "UNKNOWN_TOOL_ACTION"
                    }),
                ),
            ])),
        }
    }
}
pub(super) fn needs_repo_lock(action: &Value, body: &Value) -> Result<bool, Error> {
    let action = crate::param_changes::text::stripped(action, true)?;
    let Value::Text(points) = &action else {
        return Err(Error::Source("expected action text".into()));
    };
    let git_prefix = points.starts_with(&"git_".chars().map(u32::from).collect::<Vec<_>>());
    if git_prefix && !action.text_eq("git_log") || action.text_eq("rebuild_all") {
        return Ok(true);
    }
    if action.text_eq("shell_cmd") {
        let command = crate::param_changes::text::stripped(body.get("cmd"), true)?;
        if let Value::Text(points) = command {
            return Ok(points.starts_with(&"git ".chars().map(u32::from).collect::<Vec<_>>()));
        }
    }
    Ok(false)
}

pub(super) fn shell(argv: &[String]) -> Option<Value> {
    let failure = |error: String, code: &str, detail: &str| {
        Value::object([
            ("ok", Value::Bool(false)),
            ("error", Value::text(&error)),
            ("error_code", Value::text(code)),
            ("error_detail", Value::text(detail)),
        ])
    };
    let Some(top) = argv.first() else {
        return Some(failure("empty cmd".into(), "EMPTY_CMD", ""));
    };
    if !["cat", "df", "echo", "free", "git", "ls", "uptime"].contains(&top.as_str()) {
        return Some(failure(
            format!("not allowed: {top}"),
            "CMD_NOT_ALLOWED",
            top,
        ));
    }
    if top != "git" {
        return None;
    }
    let Some(subcommand) = argv.get(1) else {
        return Some(failure(
            "missing git subcommand".into(),
            "GIT_CMD_NOT_ALLOWED",
            "",
        ));
    };
    if ![
        "branch",
        "diff",
        "log",
        "remote",
        "rev-parse",
        "show",
        "show-ref",
        "status",
    ]
    .contains(&subcommand.as_str())
    {
        return Some(failure(
            format!("git subcommand not allowed: {subcommand}"),
            "GIT_CMD_NOT_ALLOWED",
            subcommand,
        ));
    }
    None
}
