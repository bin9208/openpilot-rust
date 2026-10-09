//! Original services/setting_profiles.py storage and restore contract.
mod clean;
mod runtime;
use crate::{
    param_changes::text,
    param_restore::{read_setting_value, Restore},
    params::Backend,
    settings::Catalog,
    Error, Value,
};
pub use runtime::{commit_url, now_iso, Creation, Git};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("{message}")]
    Rejected {
        pub_code: &'static str,
        message: &'static str,
    },
    #[error("'profile not found'")]
    NotFound,
    #[error(transparent)]
    Service(#[from] Error),
}
impl From<openpilot_carrot_navi::Error> for ProfileError {
    fn from(error: openpilot_carrot_navi::Error) -> Self {
        Self::Service(Error::from(error))
    }
}
impl ProfileError {
    pub const fn code(&self) -> Option<&'static str> {
        match self {
            Self::Rejected { pub_code, .. } => Some(pub_code),
            Self::NotFound | Self::Service(_) => None,
        }
    }
}
pub struct ProfileStore<'a> {
    path: PathBuf,
    catalog: &'a Catalog,
}
fn rejected(code: &'static str, message: &'static str) -> ProfileError {
    ProfileError::Rejected {
        pub_code: code,
        message,
    }
}

impl<'a> ProfileStore<'a> {
    pub fn new(path: &Path, catalog: &'a Catalog) -> Self {
        Self {
            path: path.into(),
            catalog,
        }
    }
    pub fn read(&self) -> Result<Value, Error> {
        clean::store(&crate::state::read(&self.path), self.catalog, false)
    }
    pub fn write(&self, data: &Value) -> Result<Value, Error> {
        let clean = clean::store(data, self.catalog, true)?;
        crate::state::write(&self.path, &clean)?;
        Ok(clean)
    }
    pub fn get(&self, id: &Value) -> Result<Option<Value>, Error> {
        let id = text::stripped(id, true)?;
        let data = self.read()?;
        let Value::Array(profiles) = data.get("profiles") else {
            return Err(Error::Source("expected profiles".into()));
        };
        Ok(profiles
            .iter()
            .find(|profile| text::equal(profile.get("id"), &id))
            .cloned())
    }
    pub fn snapshot(&self, backend: &Backend) -> Result<Value, Error> {
        let mut values = Vec::new();
        for (key, definition) in crate::json_fields::fields(&self.catalog.by_name)? {
            let name = Value::Text(key.clone()).string()?;
            let default = if definition.has("default") {
                definition.get("default").clone()
            } else {
                Value::integer(0)
            };
            values.push((key.clone(), read_setting_value(backend, &name, &default)));
        }
        Ok(Value::Object(values))
    }
    pub fn create(
        &self,
        name: &Value,
        creation: &Creation,
        backend: &Backend,
    ) -> Result<Value, ProfileError> {
        self.create_using(name, backend, || creation.clone())
    }
    pub fn create_current(
        &self,
        name: &Value,
        git: &Git,
        backend: &Backend,
    ) -> Result<Value, ProfileError> {
        self.create_using(name, backend, || Creation::capture(git))
    }
    fn create_using(
        &self,
        name: &Value,
        backend: &Backend,
        creation: impl FnOnce() -> Creation,
    ) -> Result<Value, ProfileError> {
        let name = clean::name(name)?;
        if !name.truth() {
            return Err(rejected("PROFILE_NAME_REQUIRED", "missing profile name"));
        }
        let data = self.read()?;
        let Value::Array(mut profiles) = data.get("profiles").clone() else {
            return Err(Error::Source("expected profiles".into()).into());
        };
        if profiles.len() >= 40 {
            return Err(rejected("PROFILE_LIMIT", "profile limit reached"));
        }
        let creation = creation();
        let profile = Value::object([
            ("id", Value::text(&creation.id)),
            ("name", name),
            ("created_at", Value::text(&creation.now)),
            ("updated_at", Value::text(&creation.now)),
            ("meta", creation.meta.clone()),
            (
                "values",
                clean::values(&self.snapshot(backend)?, self.catalog),
            ),
        ]);
        profiles.push(profile.clone());
        self.write(&Value::object([("profiles", Value::Array(profiles))]))?;
        Ok(profile)
    }
    pub fn update(&self, id: &Value, updates: &Value, now: &str) -> Result<Value, ProfileError> {
        let data = self.read()?;
        let Value::Array(mut profiles) = data.get("profiles").clone() else {
            return Err(Error::Source("expected profiles".into()).into());
        };
        let Some(profile) = profiles
            .iter_mut()
            .find(|profile| text::equal(profile.get("id"), id))
        else {
            return Err(ProfileError::NotFound);
        };
        if updates.has("name") {
            let name = clean::name(updates.get("name"))?;
            if !name.truth() {
                return Err(rejected("PROFILE_NAME_REQUIRED", "missing profile name"));
            }
            crate::json_fields::set(profile, "name", name)?;
        }
        if updates.has("values") {
            let values = clean::values(updates.get("values"), self.catalog);
            if !values.truth() {
                return Err(rejected("PROFILE_NO_VALUES", "no valid values to save"));
            }
            crate::json_fields::set(profile, "values", values)?;
        }
        crate::json_fields::set(profile, "updated_at", Value::text(now))?;
        let profile = profile.clone();
        self.write(&Value::object([("profiles", Value::Array(profiles))]))?;
        Ok(profile)
    }
    pub fn delete(&self, id: &Value) -> Result<(), ProfileError> {
        let data = self.read()?;
        let Value::Array(profiles) = data.get("profiles") else {
            return Err(Error::Source("expected profiles".into()).into());
        };
        let retained = profiles
            .iter()
            .filter(|profile| !text::equal(profile.get("id"), id))
            .cloned()
            .collect::<Vec<_>>();
        if retained.len() == profiles.len() {
            return Err(ProfileError::NotFound);
        }
        self.write(&Value::object([("profiles", Value::Array(retained))]))?;
        Ok(())
    }
    pub fn preview(
        &self,
        id: &Value,
        values: &Value,
        restore: &Restore<'_>,
    ) -> Result<Value, ProfileError> {
        let profile = self.get(id)?.ok_or(ProfileError::NotFound)?;
        let values = if matches!(values, Value::Null) {
            profile.get("values").clone()
        } else {
            clean::values(values, self.catalog)
        };
        Ok(restore.preview(&values, &Value::Null)?)
    }
    pub fn apply(
        &self,
        id: &Value,
        values: &Value,
        restore: &mut Restore<'_>,
    ) -> Result<Value, ProfileError> {
        let profile = self.get(id)?.ok_or(ProfileError::NotFound)?;
        let values = if matches!(values, Value::Null) {
            profile.get("values").clone()
        } else {
            clean::values(values, self.catalog)
        };
        Ok(restore.apply(&values, &Value::Null, &Value::text("profile"))?)
    }
}
