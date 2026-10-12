use crate::{json_fields::insert, state, Error, Value};
use num_traits::ToPrimitive;
use std::{collections::HashSet, path::Path};

#[derive(Clone, Copy)]
pub(crate) enum Preference {
    Units,
    Favorites,
}

impl Preference {
    pub(crate) fn path(self) -> &'static str {
        match self {
            Self::Units => "setting_unit_index.json",
            Self::Favorites => "setting_favorites.json",
        }
    }
    pub(crate) fn from_path(path: &str) -> Option<Self> {
        match path {
            "/api/setting_unit_index" => Some(Self::Units),
            "/api/setting_favorites" => Some(Self::Favorites),
            _ => None,
        }
    }
    fn sanitize(self, raw: &Value) -> Result<Value, Error> {
        match self {
            Self::Units => {
                let mut units = Vec::new();
                if let Value::Object(fields) = raw.get("units") {
                    for (key, value) in fields {
                        let name = state::trim(key);
                        if name.is_empty() {
                            continue;
                        }
                        if let Some(index) = unit(value)? {
                            insert(&mut units, name.to_vec(), Value::integer(index));
                            if units.len() >= 400 {
                                break;
                            }
                        }
                    }
                }
                Ok(Value::object([("units", Value::Object(units))]))
            }
            Self::Favorites => {
                let mut favorites = Vec::new();
                let mut seen = HashSet::new();
                if let Value::Array(items) = raw.get("favorites") {
                    for item in items {
                        let name = if !item.truth() {
                            Vec::new()
                        } else {
                            match item {
                                Value::Text(points) => points.clone(),
                                Value::Null
                                | Value::Bool(_)
                                | Value::Integer(_)
                                | Value::Float(_)
                                | Value::Array(_)
                                | Value::Object(_) => {
                                    item.string()?.chars().map(u32::from).collect()
                                }
                            }
                        };
                        let name = state::trim(&name);
                        if name.is_empty() || !seen.insert(name.to_vec()) {
                            continue;
                        }
                        favorites.push(Value::Text(name.to_vec()));
                        if favorites.len() >= 200 {
                            break;
                        }
                    }
                }
                Ok(Value::object([("favorites", Value::Array(favorites))]))
            }
        }
    }
    pub(crate) fn read(self, path: &Path) -> Result<Value, Error> {
        self.sanitize(&state::read(path))
    }
    pub(crate) fn update(self, path: &Path, update: &Value) -> Result<Value, Error> {
        let current = self.read(path)?;
        let clean = match self {
            Self::Units => {
                let Value::Object(mut units) = current.get("units").clone() else {
                    return Err(Error::Source("invalid units".into()));
                };
                let updates = update.get("units");
                if updates.truth() {
                    let Value::Object(fields) = updates else {
                        return Err(Error::Source(format!(
                            "'{}' object has no attribute 'items'",
                            updates.type_name()
                        )));
                    };
                    for (key, value) in fields {
                        let name = state::trim(key);
                        if name.is_empty() {
                            continue;
                        }
                        let key = name.to_vec();
                        match unit(value)? {
                            Some(index) => insert(&mut units, key, Value::integer(index)),
                            None if value.int().is_ok() => units.retain(|(name, _)| *name != key),
                            None => {}
                        }
                    }
                }
                self.sanitize(&Value::object([("units", Value::Object(units))]))?
            }
            Self::Favorites => self.sanitize(if update.has("favorites") {
                update
            } else {
                &current
            })?,
        };
        state::write(path, &clean)?;
        Ok(clean)
    }
}

fn unit(value: &Value) -> Result<Option<i64>, Error> {
    match value.int() {
        Ok(index) => Ok(index.to_i64().filter(|index| (1..6).contains(index))),
        Err(error) if matches!(error.kind, "TypeError" | "ValueError") => Ok(None),
        Err(error) => Err(error.into()),
    }
}
