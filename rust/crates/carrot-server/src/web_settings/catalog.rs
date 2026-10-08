//! Source: server/services/web_settings.py catalog validation.
use super::coercion::stripped;
use crate::{Error, Value};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(super) struct Content {
    pub(super) id: String,
    pub(super) label_key: Value,
    pub(super) slots: Vec<String>,
    pub(super) sources: Vec<String>,
    pub(super) singleton: bool,
}

#[derive(Clone, Debug)]
pub struct Catalog {
    pub(super) primary: String,
    pub(super) secondary: String,
    pub(super) contents: Vec<Content>,
}

fn failure(message: impl Into<String>) -> Error {
    Error::Source(message.into())
}

fn exact_keys(value: &Value, expected: &[&str], label: &str) -> Result<(), Error> {
    let crate::Value::Object(fields) = value else {
        return Err(failure(format!("{label} must be an object")));
    };
    let actual: BTreeSet<Vec<u32>> = fields.iter().map(|(key, _)| key.clone()).collect();
    let expected: BTreeSet<Vec<u32>> = expected
        .iter()
        .map(|key| key.chars().map(u32::from).collect())
        .collect();
    if actual != expected {
        let missing = Value::Array(
            expected
                .difference(&actual)
                .cloned()
                .map(Value::Text)
                .collect(),
        );
        let unknown = Value::Array(
            actual
                .difference(&expected)
                .cloned()
                .map(Value::Text)
                .collect(),
        );
        return Err(failure(format!(
            "{label} keys mismatch: missing={} unknown={}",
            missing.repr()?,
            unknown.repr()?
        )));
    }
    Ok(())
}

fn enum_list(value: &Value, allowed: &[&str], label: &str) -> Result<Vec<String>, Error> {
    let Value::Array(items) = value else {
        return Err(failure(format!("{label} must be a non-empty list")));
    };
    if items.is_empty() {
        return Err(failure(format!("{label} must be a non-empty list")));
    }
    let mut output = Vec::with_capacity(items.len());
    for item in items {
        let Some(text) = allowed.iter().find(|text| item.text_eq(text)) else {
            return Err(failure(format!("{label} contains an unsupported value")));
        };
        output.push((*text).to_owned());
    }
    if output.iter().collect::<BTreeSet<_>>().len() != output.len() {
        return Err(failure(format!("{label} contains duplicate values")));
    }
    Ok(output)
}

impl Catalog {
    pub fn validate(raw: &Value) -> Result<Self, Error> {
        exact_keys(raw, &["schemaVersion", "defaults", "contents"], "catalog")?;
        if !matches!(raw.get("schemaVersion"), Value::Integer(version) if version == &1.into()) {
            return Err(failure("catalog schemaVersion must be 1"));
        }
        let defaults = raw.get("defaults");
        exact_keys(defaults, &["primary", "secondary"], "catalog defaults")?;
        let Value::Array(contents) = raw.get("contents") else {
            return Err(failure("catalog contents must be a non-empty list"));
        };
        if contents.is_empty() {
            return Err(failure("catalog contents must be a non-empty list"));
        }
        let mut seen = BTreeSet::new();
        let mut normalized = Vec::with_capacity(contents.len());
        for (index, descriptor) in contents.iter().enumerate() {
            let label = format!("catalog contents[{index}]");
            exact_keys(
                descriptor,
                &[
                    "id",
                    "labelKey",
                    "supportedSlots",
                    "supportedSources",
                    "singleton",
                ],
                &label,
            )?;
            let valid_id = match descriptor.get("id") {
                Value::Text(points) => {
                    points
                        .first()
                        .is_some_and(|point| (97..=122).contains(point))
                        && points.iter().all(|point| {
                            (97..=122).contains(point) || (48..=57).contains(point) || *point == 95
                        })
                }
                Value::Null
                | Value::Bool(_)
                | Value::Integer(_)
                | Value::Float(_)
                | Value::Array(_)
                | Value::Object(_) => false,
            };
            if !valid_id {
                return Err(failure(format!("{label}.id is invalid")));
            }
            let id = descriptor.get("id").string()?;
            if !seen.insert(id.clone()) {
                return Err(failure(format!("duplicate catalog content id: {id}")));
            }
            let label_key = descriptor.get("labelKey");
            if !matches!(label_key, Value::Text(_)) || stripped(label_key, false)?.is_empty() {
                return Err(failure(format!(
                    "{label}.labelKey must be a non-empty string"
                )));
            }
            let Value::Bool(singleton) = descriptor.get("singleton") else {
                return Err(failure(format!("{label}.singleton must be a boolean")));
            };
            normalized.push(Content {
                id,
                label_key: label_key.clone(),
                slots: enum_list(
                    descriptor.get("supportedSlots"),
                    &["primary", "secondary"],
                    &format!("{label}.supportedSlots"),
                )?,
                sources: enum_list(
                    descriptor.get("supportedSources"),
                    &["live", "replay"],
                    &format!("{label}.supportedSources"),
                )?,
                singleton: *singleton,
            });
        }
        let mut names = Vec::with_capacity(2);
        for slot in ["primary", "secondary"] {
            let Some(id) = seen.iter().find(|id| defaults.get(slot).text_eq(id)) else {
                return Err(failure(format!(
                    "catalog defaults.{slot} must reference a content id"
                )));
            };
            names.push(id.clone());
        }
        Ok(Self {
            primary: names.remove(0),
            secondary: names.remove(0),
            contents: normalized,
        })
    }

    pub fn safe() -> Self {
        Self {
            primary: "vision".into(),
            secondary: "navigation".into(),
            contents: [
                ("vision", "web_settings_carrot_vision"),
                ("navigation", "web_settings_navigation"),
            ]
            .into_iter()
            .map(|(id, label)| Content {
                id: id.into(),
                label_key: Value::text(label),
                slots: vec!["primary".into(), "secondary".into()],
                sources: vec!["live".into(), "replay".into()],
                singleton: true,
            })
            .collect(),
        }
    }

    pub fn value(&self) -> Value {
        Value::object([
            ("schemaVersion", Value::integer(1)),
            (
                "defaults",
                Value::object([
                    ("primary", Value::text(&self.primary)),
                    ("secondary", Value::text(&self.secondary)),
                ]),
            ),
            (
                "contents",
                Value::Array(
                    self.contents
                        .iter()
                        .map(|content| {
                            Value::object([
                                ("id", Value::text(&content.id)),
                                ("labelKey", content.label_key.clone()),
                                (
                                    "supportedSlots",
                                    Value::Array(
                                        content
                                            .slots
                                            .iter()
                                            .map(|slot| Value::text(slot))
                                            .collect(),
                                    ),
                                ),
                                (
                                    "supportedSources",
                                    Value::Array(
                                        content
                                            .sources
                                            .iter()
                                            .map(|source| Value::text(source))
                                            .collect(),
                                    ),
                                ),
                                ("singleton", Value::Bool(content.singleton)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}
