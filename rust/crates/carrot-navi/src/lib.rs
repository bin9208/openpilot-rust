#![forbid(unsafe_code)]

mod integer;
pub mod json;
mod json_log;
pub mod manifest;
#[cfg(feature = "native")]
pub mod native;
pub mod packet;
pub mod projection;
pub mod receiver;
pub mod record;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
    unicode_message: Option<Vec<u32>>,
}

impl Error {
    pub fn value(message: &str) -> Self {
        Self {
            kind: "ValueError",
            message: message.to_owned(),
            unicode_message: None,
        }
    }

    pub fn typed(kind: &'static str, message: String) -> Self {
        Self {
            kind,
            message,
            unicode_message: None,
        }
    }

    pub fn value_detail(prefix: &str, value: &json::Value) -> Result<Self, Self> {
        let json::Value::Text(points) = value.py_string()? else {
            return Err(Self::value("invalid Python string conversion"));
        };
        let mut message: Vec<_> = prefix.chars().map(u32::from).collect();
        message.extend(points);
        let display = message
            .iter()
            .copied()
            .map(char::from_u32)
            .collect::<Option<String>>()
            .unwrap_or_else(|| format!("{prefix}<Python Unicode text>"));
        Ok(Self {
            kind: "ValueError",
            message: display,
            unicode_message: Some(message),
        })
    }

    pub fn message_value(&self) -> json::Value {
        self.unicode_message
            .as_ref()
            .map(|points| json::Value::Text(points.clone()))
            .unwrap_or_else(|| json::Value::text(&self.message))
    }
}
