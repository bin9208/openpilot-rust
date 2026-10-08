use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
    json_fields::{fields, set},
    param_changes::{text::equal, Change},
    param_coercion::{python_float, rounded},
    param_restore::read_setting_value,
    Error, Value,
};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use openpilot_hardware_info::HardwareInfo;
use std::sync::Arc;

fn reply(status: StatusCode, value: Value, head: bool) -> Response<Body> {
    json_response(status, value, head, "").unwrap_or_else(|_| failure(head))
}
fn reject(status: StatusCode, message: &str, head: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
        head,
    )
}
fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}
fn device_type() -> String {
    let value = if std::path::Path::new("/TICI").is_file() {
        openpilot_hardware_info::Tici::default().get_device_type()
    } else {
        openpilot_hardware_info::Pc.get_device_type()
    };
    value.unwrap_or_else(|_| "unknown".into())
}
fn bulk(app: &Application, names: &str) -> Result<Value, Error> {
    let params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    let catalog = app.catalog(&params).ok();
    let definitions = catalog
        .as_ref()
        .map_or(&Value::Null, |catalog| &catalog.by_name);
    let mut values = Value::Object(Vec::new());
    let names: Vec<_> = names.split(',').filter(|name| !name.is_empty()).collect();
    for &name in names.iter().filter(|name| **name != "DeviceType") {
        let definition = definitions.get(name);
        let default = if definition.has("default") {
            definition.get("default").clone()
        } else {
            Value::integer(0)
        };
        set(
            &mut values,
            name,
            read_setting_value(&params, name, &default),
        )?;
    }
    if names.contains(&"DeviceType") {
        set(&mut values, "DeviceType", Value::text(&device_type()))?;
    }
    app.history.observe(&values, Some(definitions))?;
    Ok(Value::object([
        ("ok", Value::Bool(true)),
        ("values", values),
    ]))
}

fn set_value(app: &Application, body: &Value) -> Result<(StatusCode, Value), Error> {
    fields(body)?;
    let name_value = body.get("name");
    if !name_value.truth() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text("missing name")),
            ]),
        ));
    }
    let name = name_value.string()?;
    let mut params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    let catalog = app.catalog(&params).ok();
    let definition = catalog
        .as_ref()
        .map(|catalog| catalog.by_name.get(&name))
        .filter(|definition| !matches!(definition, Value::Null));
    let default = definition.map_or(&Value::Null, |definition| definition.get("default"));
    let previous = read_setting_value(&params, &name, default);
    let mut value = body.get("value").clone();
    if let Some(definition) = definition {
        let bounds = [definition.get("min"), definition.get("max")];
        if bounds
            .iter()
            .all(|bound| matches!(bound, Value::Integer(_) | Value::Float(_) | Value::Bool(_)))
        {
            if let Ok(mut number) = python_float(&value) {
                if let (Ok(minimum), Ok(maximum)) = (bounds[0].float(), bounds[1].float()) {
                    if number < minimum {
                        number = minimum;
                    }
                    if number > maximum {
                        number = maximum;
                    }
                    let clamped = Value::Float(number);
                    if [bounds[0], bounds[1], definition.get("default")]
                        .iter()
                        .all(|number| matches!(number, Value::Integer(_) | Value::Bool(_)))
                    {
                        if let Ok(integer) = rounded(&clamped) {
                            value = Value::Integer(integer);
                        }
                    } else {
                        value = clamped;
                    }
                }
            }
        }
    }
    if let Err(error) = params.put(&name, &value, definition) {
        return Ok((
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::object([
                ("ok", Value::Bool(false)),
                ("error", Value::text(&error.to_string())),
            ]),
        ));
    }
    if !equal(&previous, &value) {
        app.history.append(Change {
            name: name_value,
            previous: &previous,
            next: &value,
            source: body.get("source"),
            engaged: app.drive_engaged(),
        });
    }
    Ok((
        StatusCode::OK,
        Value::object([
            ("ok", Value::Bool(true)),
            ("name", name_value.clone()),
            ("value", value),
            ("has_params", Value::Bool(params.has_params())),
        ]),
    ))
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    app: Arc<Application>,
    path: &str,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let bulk_route = path == "/api/params_bulk";
    if (bulk_route && !matches!(request.method(), &Method::GET | &Method::HEAD))
        || (!bulk_route && request.method() != Method::POST)
    {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if bulk_route {
                "GET,HEAD"
            } else {
                "GET,HEAD,POST"
            }),
        );
        return response;
    }
    let result = if bulk_route {
        let names = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
            .find(|(key, _)| key == "names")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        if names.is_empty() {
            return reject(StatusCode::BAD_REQUEST, "missing names", head);
        }
        tokio::task::spawn_blocking(move || bulk(&app, &names).map(|value| (StatusCode::OK, value)))
            .await
    } else {
        let body = match read_json(request).await {
            Ok(body) => body,
            Err(_) => return reject(StatusCode::BAD_REQUEST, "invalid json", head),
        };
        tokio::task::spawn_blocking(move || set_value(&app, &body)).await
    };
    match result {
        Ok(Ok((status, value))) => reply(status, value, head),
        Ok(Err(error)) => {
            eprintln!("Params request: {error}");
            failure(head)
        }
        Err(error) => {
            eprintln!("Params task: {error}");
            failure(head)
        }
    }
}
