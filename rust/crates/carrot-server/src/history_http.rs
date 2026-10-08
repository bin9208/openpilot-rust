use crate::{
    http::{Application, Body},
    http_response::{json_response, text},
    json_fields::{fields, set},
    param_changes::{fingerprint, text::stripped},
    param_restore::read_setting_value,
    Error, Value,
};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use num_traits::ToPrimitive;
use std::sync::Arc;

pub(crate) fn matches(path: &str, method: &Method) -> bool {
    matches!(
        path,
        "/api/param_changes" | "/api/param_changes/verify" | "/api/param_fingerprint"
    ) || (path == "/api/param_fingerprint/baseline"
        && !matches!(method, &Method::GET | &Method::HEAD))
}

fn fingerprint_payload(app: &Application, baseline_only: bool) -> Result<Value, Error> {
    let params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    let catalog = app.catalog(&params)?;
    let mut values = Vec::new();
    for (name, definition) in fields(&catalog.by_name)? {
        let name_value = Value::Text(name.clone());
        let name_text = name_value.string()?;
        let default = if definition.has("default") {
            definition.get("default").clone()
        } else {
            Value::integer(0)
        };
        values.push((
            name.clone(),
            read_setting_value(&params, &name_text, &default),
        ));
    }
    let mut result = fingerprint(&Value::Object(values))?;
    let digest = result.get("fingerprint").clone();
    if baseline_only {
        return Ok(Value::object([
            ("ok", Value::Bool(true)),
            ("baseline", app.history.write_baseline(&digest)?),
        ]));
    }
    let baseline = app
        .history
        .read_baseline()?
        .map_or_else(|| app.history.write_baseline(&digest), Ok)?;
    let changed = digest != *baseline.get("fingerprint");
    let timestamp = if baseline.get("ts").truth() {
        Value::Integer(baseline.get("ts").int()?)
    } else {
        Value::integer(0)
    };
    let count = if changed {
        app.history
            .count_since(&timestamp, Some(&catalog.by_name))?
    } else {
        0
    };
    set(&mut result, "baseline", baseline)?;
    set(&mut result, "changed", Value::Bool(changed))?;
    set(&mut result, "changed_count", Value::integer(count))?;
    let mut payload = Value::object([("ok", Value::Bool(true))]);
    if let (Value::Object(payload), Value::Object(result)) = (&mut payload, result) {
        payload.extend(result);
    }
    Ok(payload)
}

fn failed(error: &str, plain: bool, head: bool) -> Response<Body> {
    if plain {
        eprintln!("history request: {error}");
        text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500 Internal Server Error\n\nServer got itself in trouble",
            head,
        )
    } else {
        json_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            Value::object([("ok", Value::Bool(false)), ("error", Value::text(error))]),
            head,
            "",
        )
        .unwrap_or_else(|_| {
            text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500: Internal Server Error",
                head,
            )
        })
    }
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    app: Arc<Application>,
    path: &str,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let baseline = path.ends_with("/baseline");
    if (baseline && request.method() != Method::POST)
        || (!baseline && !matches!(request.method(), &Method::GET | &Method::HEAD))
    {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if baseline {
                "GET,HEAD,POST"
            } else {
                "GET,HEAD"
            }),
        );
        return response;
    }
    let path = path.to_owned();
    let query = request.uri().query().unwrap_or("").to_owned();
    let changes_route = path == "/api/param_changes";
    let verify_route = path == "/api/param_changes/verify";
    let result = tokio::task::spawn_blocking(move || {
        if changes_route {
            let parameter = |name: &str, default: &str| {
                url::form_urlencoded::parse(query.as_bytes())
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| Value::text(&value))
                    .unwrap_or_else(|| Value::text(default))
            };
            let limit = parameter("limit", "50").int().ok().map_or(50, |value| {
                if value < num_bigint::BigInt::from(0) {
                    0
                } else {
                    value.to_usize().unwrap_or(500).min(500)
                }
            });
            let name = stripped(&parameter("name", ""), false)?;
            let source = stripped(&parameter("source", ""), false)?;
            Ok(Value::object([
                ("ok", Value::Bool(true)),
                ("changes", app.history.read(limit, &name, &source)?),
            ]))
        } else if verify_route {
            app.history.verify()
        } else {
            fingerprint_payload(&app, baseline)
        }
    })
    .await;
    match result {
        Ok(Ok(value)) => json_response(StatusCode::OK, value, head, "").unwrap_or_else(|_| {
            text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
                head,
            )
        }),
        Ok(Err(error)) => failed(&error.to_string(), changes_route || verify_route, head),
        Err(error) => failed(&error.to_string(), changes_route || verify_route, head),
    }
}
