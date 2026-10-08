use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    param_qr::{Codec, Schema},
    params::Backend,
    Error, Value,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

fn bytes(value: &Value) -> Result<Vec<u8>, Error> {
    let Value::Array(values) = value else {
        return Err(Error::Source("expected bytes".into()));
    };
    values
        .iter()
        .map(|value| {
            value
                .int()?
                .to_u8()
                .ok_or_else(|| Error::Source("invalid byte".into()))
        })
        .collect()
}
fn schema(value: &Value) -> Result<Schema, Error> {
    let Value::Array(names) = value.get("names") else {
        return Err(Error::Source("expected names".into()));
    };
    let Value::Object(types) = value.get("types") else {
        return Err(Error::Source("expected types".into()));
    };
    Ok(Schema {
        names: names
            .iter()
            .map(|name| name.string().map_err(Error::from))
            .collect::<Result<_, _>>()?,
        types: types
            .iter()
            .map(|(name, kind)| {
                Ok((
                    Value::Text(name.clone()).string()?,
                    kind.int()?
                        .to_u8()
                        .ok_or_else(|| Error::Source("invalid kind".into()))?,
                ))
            })
            .collect::<Result<_, Error>>()?,
    })
}
fn run(request: &Value, backend: &Backend) -> Result<Value, Error> {
    let declared = if request.has("schema") {
        Some(schema(request.get("schema"))?)
    } else if request.get("unavailable").truth() {
        None
    } else {
        Some(Schema::from_backend(backend)?)
    };
    let codec = Codec::with_schema(declared, !request.get("absent_brotli").truth());
    match request.get("action").string()?.as_str() {
        "build" => {
            let backup;
            let values = if request.has("values") {
                request.get("values")
            } else {
                backup = backend.backup_values()?;
                &backup
            };
            if request.has("version") {
                codec.build_version(
                    values,
                    request
                        .get("version")
                        .int()?
                        .to_u8()
                        .ok_or_else(|| Error::Source("invalid version".into()))?,
                )
            } else {
                codec.build(values)
            }
        }
        "binary" => Ok(Value::Array(
            codec
                .binary(
                    request.get("values"),
                    request
                        .get("version")
                        .int()?
                        .to_u8()
                        .ok_or_else(|| Error::Source("invalid version".into()))?,
                )?
                .into_iter()
                .map(Value::integer)
                .collect(),
        )),
        "parse_binary" => codec.parse_binary(&bytes(request.get("data"))?),
        "parse" => codec.parse(request.get("data")),
        "backup" => backend.backup_values(),
        _ => Err(Error::Source("unknown QR operation".into())),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = Backend::memory(PathBuf::new());
    for line in io::stdin().lock().lines() {
        let request = Value::parse(&line?)?;
        let result = if request.get("action").text_eq("init") {
            backend = Backend::native(
                openpilot_params::Params::for_runtime_at(&PathBuf::from(
                    request.get("root").string()?,
                ))?,
                PathBuf::new(),
            );
            Ok(Value::Null)
        } else {
            run(&request, &backend)
        };
        let output = result.unwrap_or_else(|failure| {
            Value::object([("error", Value::text(&failure.to_string()))])
        });
        println!("{}", output.encode()?);
    }
    Ok(())
}
