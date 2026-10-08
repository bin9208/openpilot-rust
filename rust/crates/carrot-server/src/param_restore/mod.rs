//! Original params.py preview and validated restore paths, excluding backup codecs.
mod kind;
use crate::{
    param_changes::{text, Change, History},
    params::Backend,
    settings::Catalog,
    Error, Value,
};
use kind::Kind;
use num_traits::ToPrimitive;

pub fn read_setting_value(backend: &Backend, name: &str, default: &Value) -> Value {
    let value = backend.get(name, default);
    if name != "CruiseGapLevels" || !backend.has_params() {
        return value;
    }
    let maximum = backend.maximum_gap_levels();
    let requested = value
        .int()
        .ok()
        .and_then(|number| number.to_i64())
        .unwrap_or(0);
    Value::integer(if requested > 0 {
        requested.max(2).min(maximum)
    } else {
        maximum
    })
}

pub struct Restore<'a> {
    backend: &'a mut Backend,
    catalog: &'a Catalog,
    history: &'a History,
}
enum Status {
    Changed,
    Same,
    Skipped,
    Invalid,
}
impl Status {
    const fn name(&self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Same => "same",
            Self::Skipped => "skipped",
            Self::Invalid => "invalid",
        }
    }
    const fn index(&self) -> usize {
        match self {
            Self::Changed => 0,
            Self::Same => 1,
            Self::Skipped => 2,
            Self::Invalid => 3,
        }
    }
}

impl<'a> Restore<'a> {
    pub fn new(backend: &'a mut Backend, catalog: &'a Catalog, history: &'a History) -> Self {
        Self {
            backend,
            catalog,
            history,
        }
    }
    pub fn preview(&self, values: &Value, selected: &Value) -> Result<Value, Error> {
        if !self.backend.has_params() {
            return Err(Error::Source("Params/ParamKeyType not available".into()));
        }
        let mut fields = crate::json_fields::fields(values)?
            .iter()
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        let selected = match selected {
            Value::Null => &[][..],
            Value::Array(items) => items.as_slice(),
            _ => {
                return Err(Error::Source(format!(
                    "'{}' object is not iterable",
                    selected.type_name()
                )));
            }
        };
        let mut counts = [0usize; 4];
        let mut selected_count = 0;
        let mut entries = Vec::new();
        for (key, raw) in fields {
            let key_value = Value::Text(key.clone());
            let name = key_value.string().ok();
            let definition =
                crate::json_fields::field(crate::json_fields::fields(&self.catalog.by_name)?, key)
                    .unwrap_or(&Value::Null);
            let kind = match &name {
                Some(name) => Kind::resolve(name, definition),
                None => Kind::inferred(definition),
            };
            let mut status = Status::Changed;
            let mut reason = String::new();
            let mut can_apply = true;
            let mut value = raw.clone();
            let current = match &name {
                Some(name) => read_setting_value(self.backend, name, &Value::integer(0)),
                None => Value::integer(0),
            };
            match kind {
                Kind::Unknown => {
                    status = Status::Invalid;
                    reason = "unknown parameter".into();
                    can_apply = false;
                }
                Kind::Json | Kind::Bytes => {
                    status = Status::Skipped;
                    reason = "unsupported type".into();
                    can_apply = false;
                }
                Kind::Bool | Kind::Int | Kind::Float | Kind::String | Kind::Time => {
                    match kind.normalize(raw) {
                        Ok(normalized) => {
                            value = normalized;
                            if kind.equal(&current, &value)? {
                                status = Status::Same;
                                can_apply = false;
                            }
                        }
                        Err(error) => {
                            status = Status::Invalid;
                            reason = error.to_string();
                            can_apply = false;
                        }
                    }
                }
            }
            let apply = can_apply
                && (selected.is_empty()
                    || selected
                        .iter()
                        .any(|candidate| text::equal(candidate, &key_value)));
            if apply {
                selected_count += 1;
            }
            counts[status.index()] += 1;
            entries.push(Value::object([
                ("key", key_value),
                ("type", Value::text(kind.name())),
                ("current", current),
                ("value", value),
                ("status", Value::text(status.name())),
                ("reason", Value::text(&reason)),
                ("apply", Value::Bool(apply)),
            ]));
        }
        Ok(Value::object([
            ("count", Value::integer(entries.len())),
            (
                "summary",
                Value::object([
                    ("changed", Value::integer(counts[0])),
                    ("same", Value::integer(counts[1])),
                    ("skipped", Value::integer(counts[2])),
                    ("invalid", Value::integer(counts[3])),
                    ("selected", Value::integer(selected_count)),
                ]),
            ),
            ("entries", Value::Array(entries)),
        ]))
    }
    pub fn apply(
        &mut self,
        values: &Value,
        selected: &Value,
        source: &Value,
    ) -> Result<Value, Error> {
        let preview = self.preview(values, selected)?;
        let Value::Array(entries) = preview.get("entries") else {
            return Err(Error::Source("expected restore preview entries".into()));
        };
        let mut apply = Vec::new();
        for entry in entries.iter().filter(|entry| entry.get("apply").truth()) {
            let Value::Text(key) = entry.get("key") else {
                return Err(Error::Source("expected restore key".into()));
            };
            apply.push((key.clone(), entry.get("value").clone()));
        }
        let result = self.restore_values(&Value::Object(apply), source)?;
        Ok(Value::object([("preview", preview), ("result", result)]))
    }
    pub fn restore_values(&mut self, values: &Value, source: &Value) -> Result<Value, Error> {
        if !self.backend.has_params() {
            return Err(Error::Source("Params/ParamKeyType not available".into()));
        }
        let mut ok_count = 0usize;
        let mut failures = Vec::new();
        for (key, value) in crate::json_fields::fields(values)? {
            let key_value = Value::Text(key.clone());
            let name = key_value.string()?;
            let definition = self.catalog.by_name.get(&name);
            match Kind::resolve(&name, definition) {
                Kind::Unknown | Kind::Json | Kind::Bytes => continue,
                Kind::Bool | Kind::Int | Kind::Float | Kind::String | Kind::Time => {}
            }
            let previous = self.backend.get(&name, &Value::Null);
            let definition = definition.truth().then_some(definition);
            match self.backend.put(&name, value, definition) {
                Ok(()) => {
                    ok_count += 1;
                    let after = self.backend.get(&name, &Value::Null);
                    if !text::equal(&previous, &after) {
                        self.history.append(Change {
                            name: &key_value,
                            previous: &previous,
                            next: &after,
                            source,
                            engaged: false,
                        });
                    }
                }
                Err(error) => failures.push(Value::object([
                    ("key", key_value),
                    ("err", Value::text(&error.to_string())),
                ])),
            }
        }
        let failure_count = failures.len();
        failures.truncate(30);
        Ok(Value::object([
            ("ok_cnt", Value::integer(ok_count)),
            ("fail_cnt", Value::integer(failure_count)),
            ("fails", Value::Array(failures)),
        ]))
    }
}
