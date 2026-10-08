use super::Backend;
use crate::{Error, Value};
use openpilot_params::{metadata, KeyInfo, KEYS};
pub type BackupSchema = (Vec<String>, Vec<(String, u8)>);

fn convert(info: &KeyInfo, bytes: &[u8]) -> Option<Value> {
    if info.kind == 1 {
        return Some(Value::Bool(bytes == b"1"));
    }
    if info.kind == 6 {
        return Some(Value::text(&String::from_utf8_lossy(bytes)));
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let raw = Value::text(text);
    match info.kind {
        2 | 3 if !bytes.is_ascii() => None,
        2 => raw.int().ok().map(Value::Integer),
        3 => raw.float().ok().map(Value::Float),
        4 => crate::param_time::read(text),
        5 => Value::parse(text)
            .ok()
            .filter(|value| !matches!(value, Value::Null)),
        _ => Some(raw),
    }
}

impl Backend {
    /// The registered default converted by the original Params.get_default_value.
    pub fn registered_default(&self, name: &str) -> Option<Value> {
        self.native.as_ref()?;
        let info = metadata(name)?;
        convert(info, info.default?.as_bytes())
    }

    /// Original safe Params.get conversion for backup and intro text comparisons.
    /// BYTES are decoded with replacement, as intro._text does before comparison.
    pub fn typed_value(&self, name: &str, return_default: bool) -> Option<Value> {
        let params = self.native.as_ref()?;
        let info = metadata(name)?;
        params
            .get(name)
            .ok()
            .flatten()
            .filter(|bytes| !bytes.is_empty())
            .and_then(|bytes| convert(info, &bytes))
            .or_else(|| {
                return_default
                    .then(|| self.registered_default(name))
                    .flatten()
            })
    }

    fn backup_defaults(
        &self,
    ) -> Result<impl Iterator<Item = (&'static KeyInfo, Value)> + '_, Error> {
        if self.native.is_none() {
            return Err(Error::Source("Params/ParamKeyType not available".into()));
        }
        Ok(KEYS
            .iter()
            .filter(|info| !matches!(info.kind, 5 | 6))
            .filter_map(|info| {
                self.registered_default(info.name)
                    .map(|default| (info, default))
            }))
    }

    pub fn backup_schema(&self) -> Result<BackupSchema, Error> {
        let mut names = Vec::new();
        let mut kinds = Vec::new();
        for (info, _) in self.backup_defaults()? {
            names.push(info.name.to_owned());
            kinds.push((info.name.to_owned(), info.kind));
        }
        Ok((names, kinds))
    }

    pub fn backup_values(&self) -> Result<Value, Error> {
        let mut values = Vec::new();
        for (info, default) in self.backup_defaults()? {
            let value = self.typed_value(info.name, false).unwrap_or(default);
            values.push((
                info.name.chars().map(u32::from).collect(),
                value.py_string()?,
            ));
        }
        Ok(Value::Object(values))
    }
}
