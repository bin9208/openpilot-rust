use crate::{
    http::{Application, Body},
    http_request::read_json_detailed,
    http_response::{json_response, text},
    json_fields::fields,
    param_qr,
    param_restore::Restore,
    settings::Catalog,
    Error, Value,
};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub(crate) async fn download<T>(request: &Request<T>, path: &std::path::Path) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("GET,HEAD"));
        return response;
    }
    if !path.exists() {
        return rejected(StatusCode::NOT_FOUND, "file not found", head);
    }
    match crate::static_web::file_response(path, request).await {
        Ok(mut response) => {
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                header::HeaderValue::from_static("attachment; filename=params_backup.json"),
            );
            response
        }
        Err(_) => failure(head),
    }
}

pub(crate) fn matches(path: &str, method: &Method) -> bool {
    path == "/api/params_qr_backup"
        || (matches!(
            path,
            "/api/params_restore_preview" | "/api/params_restore_json"
        ) && !matches!(method, &Method::GET | &Method::HEAD))
}

fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}
fn reply(status: StatusCode, value: Value, head: bool) -> Response<Body> {
    json_response(status, value, head, "").unwrap_or_else(|_| failure(head))
}
fn rejected(status: StatusCode, error: &str, head: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(error))]),
        head,
    )
}
fn process(app: &Application, body: &Value, backup: bool, preview: bool) -> Result<Value, Error> {
    let mut params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    let mut response = Value::object([("ok", Value::Bool(true))]);
    if backup {
        if let (Value::Object(response), Value::Object(payload)) =
            (&mut response, param_qr::build(None, &params)?)
        {
            response.extend(payload);
        }
        return Ok(response);
    }
    fields(body)?;
    let data = if matches!(body.get("values"), Value::Object(_)) {
        body.get("values")
    } else {
        body.get("payload")
    };
    let values = param_qr::parse(data, &params)?;
    let keys = if matches!(body.get("keys"), Value::Array(_)) {
        body.get("keys")
    } else {
        &Value::Null
    };
    crate::param_restore::selected_keys(keys)?;
    let catalog = app
        .catalog(&params)
        .or_else(|_| Catalog::from_data(Value::object([("params", Value::Array(Vec::new()))])))?;
    let mut restore = Restore::new(&mut params, &catalog, &app.history);
    if preview {
        crate::json_fields::set(&mut response, "preview", restore.preview(&values, keys)?)?;
    } else if let Value::Object(result) = restore.apply(&values, keys, &Value::text("restore"))? {
        if let Value::Object(response) = &mut response {
            response.extend(result);
        }
    }
    Ok(response)
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    app: Arc<Application>,
    path: &str,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let backup = path == "/api/params_qr_backup";
    if (backup && !matches!(request.method(), &Method::GET | &Method::HEAD))
        || (!backup && request.method() != Method::POST)
    {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if backup { "GET,HEAD" } else { "GET,HEAD,POST" }),
        );
        return response;
    }
    if app.params.lock().is_ok_and(|params| !params.has_params()) {
        return rejected(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Params/ParamKeyType not available",
            head,
        );
    }
    let error_status = if backup {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    let body = if backup {
        Value::Null
    } else {
        match read_json_detailed(request).await {
            Ok(body) => body,
            Err(error) => return rejected(error_status, &error.to_string(), head),
        }
    };
    let preview = path.ends_with("_preview");
    match tokio::task::spawn_blocking(move || process(&app, &body, backup, preview)).await {
        Ok(Ok(value)) => {
            let mut response = reply(StatusCode::OK, value, head);
            if backup {
                response.headers_mut().insert(
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("no-store"),
                );
            }
            response
        }
        Ok(Err(error)) => rejected(error_status, &error.to_string(), head),
        Err(error) => rejected(error_status, &error.to_string(), head),
    }
}
