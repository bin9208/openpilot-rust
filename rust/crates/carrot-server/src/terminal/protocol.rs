use super::{
    session::{Context, Mode},
    Command, Config,
};
use crate::{tools::shell::quote, Error, Value};
use futures_util::SinkExt;
use num_traits::ToPrimitive;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

pub(super) fn text(value: &Value) -> Result<String, Error> {
    if !value.truth() {
        return Ok(String::new());
    }
    match value {
        Value::Text(points) => Ok(points
            .iter()
            .map(|point| char::from_u32(*point).unwrap_or('?'))
            .collect()),
        _ => Ok(value.string()?),
    }
}
pub(super) fn translate(config: &Config, line: &str, nested: bool) -> String {
    let stripped = line.trim();
    if let Some(command) = stripped.strip_prefix("::") {
        let command = command.trim();
        return format!(
            "{} --line {}",
            quote(&config.cli.to_string_lossy()),
            quote(if command.is_empty() { "help" } else { command })
        );
    }
    if nested {
        let words: Vec<_> = line.split_whitespace().collect();
        if words
            .first()
            .is_some_and(|word| word.eq_ignore_ascii_case("tmux"))
            && words.get(1).is_some_and(|word| {
                ["a", "attach", "attach-session"]
                    .iter()
                    .any(|variant| word.eq_ignore_ascii_case(variant))
            })
        {
            if words.len() == 2 {
                return "TMUX= tmux a -t comma".into();
            }
            if words.len() == 4 && words[2].eq_ignore_ascii_case("-t") {
                return format!("TMUX= {}", line.trim());
            }
        }
    }
    line.into()
}
pub(super) async fn handle(context: &Context, value: Value) -> Result<(), Error> {
    if matches!(context.spec.mode, Mode::Tmux) {
        return context.legacy.input(value).await;
    }
    let (reply, result) = oneshot::channel();
    let command = if value.get("type").text_eq("input") {
        let line = translate(&context.legacy.config, &text(value.get("data"))?, false);
        Command::Write {
            bytes: format!("{line}\r").into_bytes(),
            reply,
        }
    } else if value.get("type").text_eq("raw") {
        let text = text(value.get("data"))?;
        if text.is_empty() {
            return Ok(());
        }
        Command::Write {
            bytes: text.into_bytes(),
            reply,
        }
    } else if value.get("type").text_eq("resize") {
        let rows = if value.get("rows").truth() {
            value.get("rows")
        } else {
            &context.spec.rows
        };
        let cols = if value.get("cols").truth() {
            value.get("cols")
        } else {
            &context.spec.cols
        };
        cols.int()?;
        let rows = rows
            .int()?
            .clamp(8.into(), 200.into())
            .to_u16()
            .ok_or_else(|| Error::Source("PTY row range".into()))?;
        Command::Resize { rows, reply }
    } else if value.get("type").text_eq("control") {
        let action = value.get("action");
        if action.truth() && !matches!(action, Value::Text(_)) {
            return Err(Error::Source(format!(
                "'{}' object has no attribute 'strip'",
                action.type_name()
            )));
        }
        match text(action)?.trim() {
            "ctrl_c" => Command::Write {
                bytes: vec![3],
                reply,
            },
            "refresh" => Command::Write {
                bytes: vec![12],
                reply,
            },
            "detach" => Command::Write {
                bytes: vec![96, 100],
                reply,
            },
            "clear" => {
                context
                    .sender
                    .send(Command::Clear(reply))
                    .map_err(|_| Error::Source("terminal owner unavailable".into()))?;
                result
                    .await
                    .map_err(|_| Error::Source("terminal owner unavailable".into()))??;
                let (reply, result) = oneshot::channel();
                context
                    .sender
                    .send(Command::Write {
                        bytes: b"clear\r".to_vec(),
                        reply,
                    })
                    .map_err(|_| Error::Source("terminal owner unavailable".into()))?;
                return result
                    .await
                    .map_err(|_| Error::Source("terminal owner unavailable".into()))?;
            }
            _ => return Ok(()),
        }
    } else {
        return Ok(());
    };
    context
        .sender
        .send(command)
        .map_err(|_| Error::Source("terminal owner unavailable".into()))?;
    result
        .await
        .map_err(|_| Error::Source("terminal owner unavailable".into()))?
}
pub(super) async fn error(context: &Context, error: Error) -> Result<(), Error> {
    if matches!(context.spec.mode, Mode::Tmux) {
        return context.legacy.error(&error).await;
    }
    let payload = Value::object([
        ("type", Value::text("error")),
        ("error", Value::text(&error.to_string())),
        ("session", Value::text("login-shell")),
    ]);
    context
        .sink
        .lock()
        .await
        .send(Message::text(payload.encode()?))
        .await
        .map_err(|error| Error::Source(error.to_string()))
}
