//! Python JSON values retain key order, arbitrary integers and escaped surrogate code units.
use indexmap::IndexMap;
use std::fmt::{self, Write};

#[derive(Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(String),
    Float(f64),
    Text(Vec<u8>),
    Array(Vec<Value>),
    Object(IndexMap<Vec<u8>, Value>),
}

enum WriteValue<'a> {
    Value(&'a Value),
    Text(&'a [u8]),
    Separator(&'static str),
}
impl Value {
    pub fn to_json(&self) -> Result<String, fmt::Error> {
        let mut output = String::new();
        let mut pending = vec![WriteValue::Value(self)];
        while let Some(action) = pending.pop() {
            match action {
                WriteValue::Text(value) => quoted(value, &mut output)?,
                WriteValue::Separator(value) => output.push_str(value),
                WriteValue::Value(value) => match value {
                    Self::Null => output.push_str("null"),
                    Self::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
                    Self::Integer(value) => output.push_str(value),
                    Self::Float(value) => {
                        openpilot_runtime_core::python_float::write_float(*value, &mut output)?
                    }
                    Self::Text(value) => quoted(value, &mut output)?,
                    Self::Array(values) => {
                        output.push('[');
                        pending.push(WriteValue::Separator("]"));
                        for (index, value) in values.iter().enumerate().rev() {
                            pending.push(WriteValue::Value(value));
                            if index != 0 {
                                pending.push(WriteValue::Separator(", "));
                            }
                        }
                    }
                    Self::Object(values) => {
                        output.push('{');
                        pending.push(WriteValue::Separator("}"));
                        for (index, (key, value)) in values.iter().enumerate().rev() {
                            pending.push(WriteValue::Value(value));
                            pending.push(WriteValue::Separator(": "));
                            pending.push(WriteValue::Text(key));
                            if index != 0 {
                                pending.push(WriteValue::Separator(", "));
                            }
                        }
                    }
                },
            }
        }
        Ok(output)
    }
    fn drain_children(&mut self, pending: &mut Vec<Self>) {
        match self {
            Self::Array(values) => pending.append(values),
            Self::Object(fields) => pending.extend(fields.drain(..).map(|(_, value)| value)),
            Self::Null | Self::Bool(_) | Self::Integer(_) | Self::Float(_) | Self::Text(_) => {}
        }
    }
}
impl Drop for Value {
    fn drop(&mut self) {
        let mut pending = Vec::new();
        self.drain_children(&mut pending);
        while let Some(mut value) = pending.pop() {
            value.drain_children(&mut pending);
        }
    }
}

fn quoted(bytes: &[u8], output: &mut String) -> fmt::Result {
    output.push('"');
    let mut bytes = bytes.iter().copied();
    while let Some(first) = bytes.next() {
        // serde_json's byte-string decoder validates escapes and returns UTF-8/WTF-8.
        let (mut point, remaining) = match first {
            0..=0x7f => (u32::from(first), 0),
            0xc2..=0xdf => (u32::from(first & 0x1f), 1),
            0xe0..=0xef => (u32::from(first & 0x0f), 2),
            0xf0..=0xf4 => (u32::from(first & 7), 3),
            _ => return Err(fmt::Error),
        };
        for _ in 0..remaining {
            point = (point << 6) | u32::from(bytes.next().ok_or(fmt::Error)? & 0x3f);
        }
        match point {
            8 => output.push_str("\\b"),
            9 => output.push_str("\\t"),
            10 => output.push_str("\\n"),
            12 => output.push_str("\\f"),
            13 => output.push_str("\\r"),
            34 => output.push_str("\\\""),
            92 => output.push_str("\\\\"),
            32..=126 => output.push(char::from_u32(point).ok_or(fmt::Error)?),
            0..=0xffff => write!(output, "\\u{point:04x}")?,
            _ => {
                let pair = point - 0x10000;
                write!(
                    output,
                    "\\u{:04x}\\u{:04x}",
                    0xd800 + (pair >> 10),
                    0xdc00 + (pair & 0x3ff)
                )?;
            }
        }
    }
    output.push('"');
    Ok(())
}
