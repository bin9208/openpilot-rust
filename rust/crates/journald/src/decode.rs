//! Structural JSON reader using serde_json for number grammar and WTF-8 string decoding.
use crate::value::Value;
use indexmap::IndexMap;
use serde::{de::Visitor, Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("JSON syntax at byte {offset}: {detail}")]
    Syntax { offset: usize, detail: String },
    #[error("JSON exceeds the Python integer conversion limit of 4300 digits")]
    IntegerLimit,
}

struct Bytes(Vec<u8>);
impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ByteVisitor;
        impl Visitor<'_> for ByteVisitor {
            type Value = Bytes;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON string")
            }
            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(bytes.to_vec()))
            }
        }
        deserializer.deserialize_bytes(ByteVisitor)
    }
}

enum Frame {
    Array(Vec<Value>),
    Object(IndexMap<Vec<u8>, Value>, Vec<u8>),
}
impl Frame {
    fn append(&mut self, value: Value) {
        match self {
            Self::Array(values) => values.push(value),
            Self::Object(fields, key) => {
                fields.insert(std::mem::take(key), value);
            }
        }
    }
    fn closing(&self) -> u8 {
        match self {
            Self::Array(_) => b']',
            Self::Object(_, _) => b'}',
        }
    }
    fn finish(self) -> Value {
        match self {
            Self::Array(values) => Value::Array(values),
            Self::Object(fields, _) => Value::Object(fields),
        }
    }
}

pub(crate) fn decode(text: &str) -> Result<Value, Error> {
    let mut reader = Reader { text, offset: 0 };
    let mut frames = Vec::new();
    loop {
        reader.space();
        let mut value = match reader.text.as_bytes().get(reader.offset) {
            Some(b'[') => {
                reader.offset += 1;
                let frame = Frame::Array(Vec::new());
                if reader.take(b']') {
                    frame.finish()
                } else {
                    frames.push(frame);
                    continue;
                }
            }
            Some(b'{') => {
                reader.offset += 1;
                if reader.take(b'}') {
                    Value::Object(IndexMap::new())
                } else {
                    frames.push(Frame::Object(IndexMap::new(), reader.key()?));
                    continue;
                }
            }
            _ => reader.value()?,
        };
        loop {
            let Some(mut frame) = frames.pop() else {
                reader.space();
                return if reader.offset == text.len() {
                    Ok(value)
                } else {
                    Err(reader.error("trailing data"))
                };
            };
            frame.append(value);
            if reader.take(frame.closing()) {
                value = frame.finish();
                continue;
            }
            if !reader.take(b',') {
                return Err(reader.error("expected comma"));
            }
            match &mut frame {
                Frame::Object(_, key) => *key = reader.key()?,
                Frame::Array(_) => {}
            }
            frames.push(frame);
            break;
        }
    }
}

struct Reader<'a> {
    text: &'a str,
    offset: usize,
}
impl Reader<'_> {
    fn error(&self, detail: impl ToString) -> Error {
        Error::Syntax {
            offset: self.offset,
            detail: detail.to_string(),
        }
    }
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.offset)
            .is_some_and(|v| b" \t\r\n".contains(v))
        {
            self.offset += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.text.as_bytes().get(self.offset) == Some(&byte) {
            self.offset += 1;
            true
        } else {
            false
        }
    }
    fn scalar<'de, T: Deserialize<'de>>(&'de mut self) -> Result<T, Error> {
        let mut stream =
            serde_json::Deserializer::from_str(&self.text[self.offset..]).into_iter::<T>();
        let value = stream
            .next()
            .ok_or_else(|| self.error("missing value"))?
            .map_err(|error| self.error(error))?;
        self.offset += stream.byte_offset();
        Ok(value)
    }
    fn string(&mut self) -> Result<Vec<u8>, Error> {
        let offset = self.offset;
        // RawValue enforces JSON control-character rules; the byte decoder alone permits them.
        let raw = self.scalar::<&RawValue>()?.get();
        serde_json::from_str::<Bytes>(raw)
            .map(|bytes| bytes.0)
            .map_err(|error| Error::Syntax {
                offset,
                detail: error.to_string(),
            })
    }
    fn key(&mut self) -> Result<Vec<u8>, Error> {
        self.space();
        if self.text.as_bytes().get(self.offset) != Some(&b'"') {
            return Err(self.error("object key must be a string"));
        }
        let key = self.string()?;
        if !self.take(b':') {
            return Err(self.error("expected colon"));
        }
        Ok(key)
    }
    fn value(&mut self) -> Result<Value, Error> {
        match self.text.as_bytes().get(self.offset).copied() {
            Some(b'"') => self.string().map(Value::Text),
            Some(b'N') if self.text[self.offset..].starts_with("NaN") => {
                self.offset += 3;
                Ok(Value::Float(f64::NAN))
            }
            Some(b'I') if self.text[self.offset..].starts_with("Infinity") => {
                self.offset += 8;
                Ok(Value::Float(f64::INFINITY))
            }
            Some(b'-') if self.text[self.offset..].starts_with("-Infinity") => {
                self.offset += 9;
                Ok(Value::Float(f64::NEG_INFINITY))
            }
            _ => {
                let raw = self.scalar::<&RawValue>()?.get();
                match raw {
                    "null" => Ok(Value::Null),
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    number if number.contains(['.', 'e', 'E']) => number
                        .parse::<f64>()
                        .map(Value::Float)
                        .map_err(|error| Error::Syntax {
                            offset: 0,
                            detail: error.to_string(),
                        }),
                    number if number.trim_start_matches('-').len() > 4300 => {
                        Err(Error::IntegerLimit)
                    }
                    "-0" => Ok(Value::Integer("0".into())),
                    number => Ok(Value::Integer(number.into())),
                }
            }
        }
    }
}
