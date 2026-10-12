//! Python string codepoints survive JSON persistence, including escaped lone surrogates.
use crate::{Error, Value};

#[derive(Clone, Default)]
pub(super) struct Key(Vec<u32>);
impl Key {
    pub fn from_value(value: &Value) -> Result<Self, Error> {
        let value = if value.truth() {
            value.py_string()?
        } else {
            Value::text("")
        };
        let Value::Text(points) = value else {
            return Err(Error::Source("expected string key".into()));
        };
        Ok(Self(crate::state::trim(&points).to_vec()))
    }
    pub fn extract(value: &Value) -> Result<Self, Error> {
        let mut key = Self::from_value(value)?;
        if key
            .0
            .iter()
            .take(7)
            .copied()
            .eq("rtmp://".chars().map(u32::from))
            || key
                .0
                .iter()
                .take(8)
                .copied()
                .eq("rtmps://".chars().map(u32::from))
        {
            let end = key
                .0
                .iter()
                .rposition(|point| *point != u32::from('/'))
                .map_or(0, |i| i + 1);
            let start = key.0[..end]
                .iter()
                .rposition(|point| *point == u32::from('/'))
                .map_or(0, |i| i + 1);
            key.0 = crate::state::trim(&key.0[start..end]).to_vec();
        }
        Ok(key)
    }
    pub fn configured(&self) -> bool {
        !self.0.is_empty()
    }
    pub fn value(&self) -> Value {
        Value::Text(self.0.clone())
    }
    pub fn mask(&self) -> Value {
        if self.0.len() <= 8 {
            Value::Text(vec![u32::from('*'); self.0.len()])
        } else {
            Value::Text([&self.0[..4], &[46, 46, 46], &self.0[self.0.len() - 4..]].concat())
        }
    }
    pub fn ingest(&self, base: &str) -> Value {
        let mut points: Vec<_> = format!("{base}/").chars().map(u32::from).collect();
        let Value::Text(mask) = self.mask() else {
            return Value::Null;
        };
        points.extend(mask);
        Value::Text(points)
    }
    pub fn url(&self, base: &str) -> Result<String, Error> {
        let value = self.value().string()?;
        Ok(format!("{base}/{value}"))
    }
    pub fn validate(&self) -> (bool, &'static str) {
        if self.0.is_empty() {
            (false, "stream key is required")
        } else if self.0.len() < 8 {
            (false, "stream key is too short")
        } else if self.0.len() > 256 {
            (false, "stream key is too long")
        } else if self.0.iter().any(|point| {
            char::from_u32(*point)
                .is_some_and(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        }) {
            (false, "stream key must not contain spaces")
        } else if !self.0.iter().all(|point| {
            u8::try_from(*point)
                .is_ok_and(|byte| byte.is_ascii_alphanumeric() || b"._/-".contains(&byte))
        }) {
            (false, "stream key contains unsupported characters")
        } else {
            (true, "format looks valid")
        }
    }
}
