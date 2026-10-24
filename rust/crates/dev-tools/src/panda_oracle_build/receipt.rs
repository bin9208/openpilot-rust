//! Small receipt tree retaining filesystem text until JSON escaping at the sink.
use crate::{python_fs, seal::Result};
use std::{collections::BTreeMap, ffi::OsString};
#[derive(Clone)]
pub(super) enum Receipt {
    Text(OsString),
    Bool(bool),
    Int(i32),
    Array(Vec<Receipt>),
    Object(BTreeMap<OsString, Receipt>),
}
impl Receipt {
    pub fn object(items: impl IntoIterator<Item = (OsString, Receipt)>) -> Self {
        Self::Object(items.into_iter().collect())
    }
    pub fn json(&self, pretty: bool) -> Result<String> {
        self.format(pretty, 0)
    }
    fn format(&self, pretty: bool, depth: usize) -> Result<String> {
        match self {
            Self::Text(value) => python_fs::json_string(value),
            Self::Bool(value) => Ok(value.to_string()),
            Self::Int(value) => Ok(value.to_string()),
            Self::Array(values) => {
                let parts = values
                    .iter()
                    .map(|v| v.format(pretty, depth + 1))
                    .collect::<Result<Vec<_>>>()?;
                Ok(join("[", "]", parts, pretty, depth))
            }
            Self::Object(values) => {
                let parts = values
                    .iter()
                    .map(|(key, value)| {
                        Ok(format!(
                            "{}{}{}",
                            python_fs::json_string(key)?,
                            if pretty { ": " } else { ":" },
                            value.format(pretty, depth + 1)?
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(join("{", "}", parts, pretty, depth))
            }
        }
    }
}
fn join(open: &str, close: &str, parts: Vec<String>, pretty: bool, depth: usize) -> String {
    if parts.is_empty() {
        return format!("{open}{close}");
    }
    if pretty {
        format!(
            "{open}\n{}\n{}{close}",
            parts
                .iter()
                .map(|v| format!("{}{v}", "  ".repeat(depth + 1)))
                .collect::<Vec<_>>()
                .join(",\n"),
            "  ".repeat(depth)
        )
    } else {
        format!("{open}{}{close}", parts.join(","))
    }
}
impl From<OsString> for Receipt {
    fn from(value: OsString) -> Self {
        Self::Text(value)
    }
}
impl From<String> for Receipt {
    fn from(value: String) -> Self {
        Self::Text(value.into())
    }
}
impl From<&str> for Receipt {
    fn from(value: &str) -> Self {
        Self::Text(value.into())
    }
}
impl From<bool> for Receipt {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<i32> for Receipt {
    fn from(value: i32) -> Self {
        Self::Int(value)
    }
}
impl From<Vec<OsString>> for Receipt {
    fn from(value: Vec<OsString>) -> Self {
        Self::Array(value.into_iter().map(Self::Text).collect())
    }
}
