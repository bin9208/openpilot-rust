use openpilot_logging::{
    record::{Level, Record},
    site::Site,
    Value as LogValue,
};
use serde_json::Value;
use std::{backtrace::Backtrace, fmt};

#[derive(Debug, Clone)]
pub enum Event {
    Fields {
        site: Site,
        name: &'static str,
        fields: Value,
    },
    Text {
        site: Site,
        level: Level,
        message: String,
        exception: Option<String>,
    },
}
impl Event {
    pub fn text(site: Site, level: Level, message: String) -> Self {
        Self::Text {
            site,
            level,
            message,
            exception: None,
        }
    }
    pub fn exception(site: Site, message: &str, error: &impl fmt::Debug) -> Self {
        let [_, details] = error_details(site, error);
        Self::Text {
            site,
            level: Level::Error,
            message: message.into(),
            exception: Some(details),
        }
    }
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Fields { name, .. } => Some(name),
            Self::Text {
                level: Level::Error,
                message,
                ..
            } => Some(message),
            Self::Text { .. } => None,
        }
    }
    pub fn into_record(self) -> Result<(Site, Record), openpilot_logging::Error> {
        match self {
            Self::Fields { site, name, fields } => {
                let LogValue::Object(fields) = LogValue::from_json(fields)? else {
                    return Err(openpilot_logging::Error::Contract(
                        "uploader event fields must be an object",
                    ));
                };
                Ok((site, Record::event(name, Vec::new(), fields)?))
            }
            Self::Text {
                site,
                level,
                message,
                exception,
            } => {
                let mut record = Record::text(level, message);
                record.exception = exception;
                Ok((site, record))
            }
        }
    }
}
pub trait EventSink {
    fn emit(&mut self, event: Event);
}
impl EventSink for Vec<Event> {
    fn emit(&mut self, event: Event) {
        self.push(event);
    }
}
pub(crate) fn error_details(site: Site, error: &impl fmt::Debug) -> [String; 2] {
    [
        format!("{error:?}"),
        format!(
            "Rust error: {error:?}\nReported at {}:{} ({})\nRust backtrace captured at the reporting site:\n{}",
            site.file,
            site.line,
            site.function,
            Backtrace::force_capture()
        ),
    ]
}

pub(crate) fn python_text(value: &LogValue) -> Result<String, fmt::Error> {
    Ok(match value {
        LogValue::Null => "None".into(),
        LogValue::Bool(value) => if *value { "True" } else { "False" }.into(),
        LogValue::Integer(value) => value.to_string(),
        LogValue::Float(value) => {
            let mut output = String::new();
            openpilot_runtime_core::python_float::write_float(*value, &mut output)?;
            output
        }
        LogValue::Text(value) => value.clone(),
        LogValue::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(python_repr)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        LogValue::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| Ok(format!(
                    "{}: {}",
                    python_repr(&LogValue::Text(key.clone()))?,
                    python_repr(value)?
                )))
                .collect::<Result<Vec<_>, fmt::Error>>()?
                .join(", ")
        ),
    })
}
fn python_repr(value: &LogValue) -> Result<String, fmt::Error> {
    if let LogValue::Text(value) = value {
        let quote = if value.contains('\'') && !value.contains('"') {
            '"'
        } else {
            '\''
        };
        let mut output = String::new();
        output.push(quote);
        for character in value.chars() {
            match character {
                '\\' => output.push_str("\\\\"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                ch if ch == quote => {
                    output.push('\\');
                    output.push(ch);
                }
                ch if ch.is_control() || ch.escape_debug().to_string().starts_with("\\u{") => {
                    use std::fmt::Write;
                    let point = u32::from(ch);
                    if point <= 0xff {
                        write!(output, "\\x{point:02x}")?;
                    } else if point <= 0xffff {
                        write!(output, "\\u{point:04x}")?;
                    } else {
                        write!(output, "\\U{point:08x}")?;
                    }
                }
                ch => output.push(ch),
            }
        }
        output.push(quote);
        Ok(output)
    } else {
        python_text(value)
    }
}
