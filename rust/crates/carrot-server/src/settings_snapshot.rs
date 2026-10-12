//! Original features/settings.py settings snapshot composition.
use crate::{
    http::Application,
    json_fields::{array, fields, key},
    param_restore::read_setting_value,
    setting_profiles::ProfileStore,
    state_preferences::Preference,
    Error, Value,
};
use std::collections::HashSet;

pub(crate) fn payload(app: &Application) -> Result<Value, Error> {
    let params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    let _fresh_params = crate::system::fresh::reopen(params.native_params())?;
    let original = app.catalog(&params)?;
    let visible = original.for_brand(&params.vehicle_brand())?;
    let mut names = HashSet::new();
    let mut values = Vec::new();
    for (_, items) in fields(&visible.groups)? {
        for item in array(items)? {
            if !item.get("name").truth() {
                continue;
            }
            let name = key(item.get("name"))?;
            if !names.insert(name.clone()) {
                continue;
            }
            let definition = original.by_name.get(&Value::Text(name.clone()).string()?);
            let default = if definition.has("default") {
                definition.get("default").clone()
            } else {
                Value::integer(0)
            };
            values.push((
                name,
                read_setting_value(&params, &item.get("name").string()?, &default),
            ));
        }
    }
    let values = Value::Object(values);
    app.history.observe(&values, None)?;
    let ssh = crate::ssh_keys::status(&params)?;
    let favorites =
        Preference::Favorites.read(&app.config.state.join(Preference::Favorites.path()))?;
    let favorites = Value::Array(
        array(favorites.get("favorites"))?
            .iter()
            .filter(|name| matches!(name, Value::Text(points) if names.contains(points)))
            .cloned()
            .collect(),
    );
    let profiles =
        ProfileStore::new(&app.config.state.join("setting_profiles.json"), &original).read()?;
    let units = Preference::Units.read(&app.config.state.join(Preference::Units.path()))?;
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        (
            "settings",
            visible.payload(&app.config.settings, params.has_params()),
        ),
        ("values", values),
        (
            "device_values",
            crate::settings_snapshot_device::values(&params, &ssh)?,
        ),
        ("device_groups", crate::settings_snapshot_device::groups()),
        ("device_network", app.system.network.snapshot(&params)?),
        ("device_ssh", ssh),
        ("favorites", favorites),
        ("profiles", profiles.get("profiles").clone()),
        (
            "popular",
            app.popular_values.read(&params, Some(&original))?,
        ),
        ("unit_index", units.get("units").clone()),
    ]))
}
