use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    dashcam::{
        catalog::{self, Catalog},
        paths::{self, Paths},
        read_state::{self, ReadState},
        Failure,
    },
    Value,
};
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};

fn key(value: catalog::RouteKey) -> Vec<Value> {
    vec![
        Value::integer(value.0),
        Value::integer(value.1),
        Value::Text(value.2),
        Value::Text(value.3),
    ]
}
fn operation(
    input: &Value,
    catalog: &Catalog,
    paths: &Paths,
    state: &ReadState,
) -> Result<Value, Failure> {
    let array = |name| -> Result<&Vec<Value>, Failure> {
        match input.get(name) {
            Value::Array(values) => Ok(values),
            _ => {
                Err(openpilot_carrot_server::Error::Source("expected fixture array".into()).into())
            }
        }
    };
    let directory = || {
        input
            .get("directory")
            .string()
            .map(PathBuf::from)
            .map_err(openpilot_carrot_server::Error::from)
            .map_err(Failure::from)
    };
    let integer = |name| {
        input
            .get(name)
            .int()
            .map_err(openpilot_carrot_server::Error::from)
            .and_then(|value| {
                value.to_i64().ok_or_else(|| {
                    openpilot_carrot_server::Error::Source("fixture integer out of range".into())
                })
            })
            .map_err(Failure::from)
    };
    Ok(
        match input
            .get("operation")
            .string()
            .map_err(openpilot_carrot_server::Error::from)?
            .as_str()
        {
            "routes" => Value::Array(catalog.build_routes()?),
            "complete" => Value::Bool(catalog.segment_is_complete(input.get("segment"))?),
            "invalidate" => {
                catalog.invalidate()?;
                Value::Null
            }
            "times" => catalog.compute_segment_times(array("segments")?, input.get("seed"))?,
            "bounds" => {
                let (start, end) = catalog.route_time_bounds(array("segments")?)?;
                Value::Array(vec![Value::integer(start), Value::integer(end)])
            }
            "end_epoch" => Value::integer(catalog::source_video_end_epoch(&directory()?)),
            "video" | "rlog" | "qlog" => {
                let directory = directory()?;
                let (path, name) = match input
                    .get("operation")
                    .string()
                    .map_err(openpilot_carrot_server::Error::from)?
                    .as_str()
                {
                    "video" => catalog::source_video(&directory)?,
                    "rlog" => catalog::source_rlog(&directory)?,
                    "qlog" => catalog::source_qlog(&directory)?,
                    _ => {
                        return Err(openpilot_carrot_server::Error::Source(
                            "invalid source fixture".into(),
                        )
                        .into())
                    }
                };
                Value::Array(vec![
                    Value::text(&path.to_string_lossy()),
                    Value::text(&name),
                ])
            }
            "summary" => Value::Array(catalog::segment_file_summary(&directory()?)?),
            "route_key" => Value::Array(key(catalog::route_creation_key(input.get("value"))?)),
            "segment_key" => {
                let (route, index, name) = catalog::segment_creation_key(input.get("value"))?;
                let mut route = key(route);
                route.extend([Value::Integer(index), Value::Text(name)]);
                Value::Array(route)
            }
            "safe_segment" => paths::safe_segment(input.get("value"))?,
            "index" => Value::Integer(paths::segment_index(input.get("value"))),
            "route_name" => paths::route_name(input.get("value"))?,
            "date" => paths::route_date_label(input.get("value")),
            "size" => Value::text(&paths::file_size_label(input.get("value"))?),
            "relative" => Value::text(&paths::relative_time(integer("epoch")?, integer("now")?)),
            "segment_dir" => Value::text(&paths.segment_dir(input.get("value"))?.to_string_lossy()),
            "cache_path" => Value::text(
                &paths
                    .cache_path(
                        &input
                            .get("kind")
                            .string()
                            .map_err(openpilot_carrot_server::Error::from)?,
                        input.get("value"),
                        &input
                            .get("extension")
                            .string()
                            .map_err(openpilot_carrot_server::Error::from)?,
                    )?
                    .to_string_lossy(),
            ),
            "normalize" => read_state::normalize_recent_segment(input.get("value"))?,
            "read" => state.read()?,
            "write" => state.write(input.get("value"), integer("now")?)?,
            _ => {
                return Err(openpilot_carrot_server::Error::Source(
                    "unknown fixture operation".into(),
                )
                .into())
            }
        },
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut lines = io::stdin().lock().lines();
    let config = Value::parse(&lines.next().ok_or("missing fixture config")??)?;
    let root = PathBuf::from(config.get("root").string()?);
    let catalog = Catalog::new(root.clone());
    let paths = Paths::new(root, PathBuf::from(config.get("cache").string()?));
    let state = ReadState::new(PathBuf::from(config.get("state").string()?));
    for line in lines {
        let line = line?;
        if line.is_empty() {
            break;
        }
        let input = Value::parse(&line)?;
        let output = match operation(&input, &catalog, &paths, &state) {
            Ok(value) => Value::object([("value", value)]),
            Err(error) => {
                let status = match &error {
                    Failure::Http { status, .. } => Value::integer(*status),
                    Failure::InvalidRecent | Failure::Runtime(_) => Value::Null,
                };
                let value_error = matches!(&error, Failure::InvalidRecent)
                    || matches!(&error, Failure::Runtime(openpilot_carrot_server::Error::Json(error)) if matches!(error.kind, "ValueError" | "UnicodeEncodeError" | "UnicodeDecodeError"));
                Value::object([
                    ("error", Value::text(&error.to_string())),
                    ("status", status),
                    ("valueError", Value::Bool(value_error)),
                ])
            }
        };
        println!("{}", output.encode()?);
        io::stdout().flush()?;
    }
    Ok(())
}
