use openpilot_carrot_server::{
    web_settings::{self, Catalog, WebSettings},
    Error, Value,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

fn apply(request: &Value) -> Result<Value, Error> {
    let settings_path = PathBuf::from(request.get("settings_path").string()?);
    let catalog_path = PathBuf::from(request.get("catalog_path").string()?);
    let store = WebSettings::new(&settings_path, &catalog_path);
    match request.get("action").string()?.as_str() {
        "validate" => Ok(Catalog::validate(request.get("data"))?.value()),
        "layout" => {
            let catalog = if request.has("catalog") {
                Catalog::validate(request.get("catalog"))?
            } else {
                store.catalog()
            };
            let source = if request.has("source") {
                request.get("source").string()?
            } else {
                "live".into()
            };
            let areas = catalog
                .normalize_layout([request.get("area_1"), request.get("area_2")], &source)?;
            Ok(Value::Array(
                areas.into_iter().map(|area| Value::text(&area)).collect(),
            ))
        }
        "sanitize" => {
            let catalog = if request.has("catalog") {
                Catalog::validate(request.get("catalog"))?
            } else {
                store.catalog()
            };
            web_settings::sanitize(request.get("data"), &catalog)
        }
        "spec" => Ok(store.client_spec()),
        "defaults" => Ok(web_settings::defaults()),
        "capability_spec" => Ok(web_settings::capability_client_spec()),
        "capability_defaults" => Ok(web_settings::defaults_for_capability(
            &request.get("id").string()?,
        )),
        "capabilities" => Ok(web_settings::resolve_capabilities(request.get("data"))),
        "known_capability" => Ok(Value::Bool(web_settings::is_known_capability(
            &request.get("id").string()?,
        ))),
        "read" => store.read(),
        "write" => store.write(request.get("data")),
        "update" => store.update(request.get("data")),
        "set_capability" => {
            store.set_capability(&request.get("id").string()?, request.get("enabled").truth())
        }
        "load" => Ok(store.catalog().value()),
        "clear" => {
            web_settings::clear_catalog_cache(Some(&catalog_path))?;
            Ok(Value::Null)
        }
        _ => Err(Error::Source("unknown web settings operation".into())),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let result = Value::parse(&line?)
            .map_err(Error::from)
            .and_then(|request| apply(&request));
        let payload = match result {
            Ok(value) => value,
            Err(Error::Io(_)) => Value::object([("error", Value::text("filesystem failure"))]),
            Err(error) => Value::object([("error", Value::text(&error.to_string()))]),
        };
        println!("{}", payload.encode()?);
    }
    Ok(())
}
