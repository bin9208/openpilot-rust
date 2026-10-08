use openpilot_carrot_server::{params, settings::Catalog, Error, Value};
use std::io::{self, BufRead};

fn apply(request: &Value) -> Result<Value, Error> {
    match request.get("action").string()?.as_str() {
        "catalog" => {
            let maximum = request
                .get("maximum")
                .int()?
                .to_string()
                .parse::<i64>()
                .map_err(|_| Error::Source("invalid gap maximum".into()))?;
            let catalog = Catalog::from_data(request.get("data").clone())?
                .with_gap_limits(maximum)?
                .for_brand(&request.get("brand").string()?)?;
            Ok(Value::object([
                ("data", catalog.data),
                ("groups", catalog.groups),
                ("by_name", catalog.by_name),
                ("groups_list", catalog.groups_list),
                ("categories", catalog.categories),
            ]))
        }
        "coerce" => {
            let (kind, coerced) =
                params::coerce_inferred(request.get("value"), request.get("setting"))?;
            Ok(Value::object([
                ("kind", Value::text(kind)),
                ("coerced", coerced),
            ]))
        }
        "get_param" | "put_param" | "get_backup" => {
            let root = std::path::PathBuf::from(request.get("root").string()?);
            let native = openpilot_params::Params::for_runtime_at(&root)?;
            let mut backend = params::Backend::native(native, root.join("state"));
            let name = request.get("name").string()?;
            if request.get("action").text_eq("get_backup") {
                backend.backup_values()
            } else if request.get("action").text_eq("get_param") {
                Ok(backend.get(&name, request.get("default")))
            } else {
                let setting = (!matches!(request.get("setting"), Value::Null))
                    .then(|| request.get("setting"));
                backend.put(&name, request.get("value"), setting)?;
                Ok(Value::Null)
            }
        }
        _ => Err(Error::Source("unknown policy operation".into())),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let line = line?;
        let result = Value::parse(&line)
            .map_err(Error::from)
            .and_then(|request| apply(&request));
        let payload = result
            .unwrap_or_else(|error| Value::object([("error", Value::text(&error.to_string()))]));
        println!("{}", payload.encode()?);
    }
    Ok(())
}
