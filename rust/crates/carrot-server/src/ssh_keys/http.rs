use super::{apply_download, clear, refresh_username, status, KeyError, Online, Username};
use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{body::Incoming, header, Method, Request, Response, StatusCode};
use std::sync::Arc;

fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}
fn reply(status: u16, value: Value, head: bool) -> Response<Body> {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    json_response(status, value, head, "").unwrap_or_else(|_| failure(head))
}
fn rejected(status: u16, error: &str, head: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(error))]),
        head,
    )
}
fn successful(value: Value, head: bool) -> Response<Body> {
    let mut response = Value::object([("ok", Value::Bool(true))]);
    if let (Value::Object(response), Value::Object(value)) = (&mut response, value) {
        response.extend(value);
    }
    reply(200, response, head)
}

pub async fn handle(
    request: Request<Incoming>,
    app: Arc<Application>,
    online: Option<Arc<Online>>,
    timestamp: Option<i64>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    if matches!(request.method(), &Method::GET | &Method::HEAD) {
        return match app
            .params
            .lock()
            .ok()
            .and_then(|params| status(&params).ok())
        {
            Some(value) => successful(value, head),
            None => failure(head),
        };
    }
    if request.method() != Method::POST {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static("GET,HEAD,POST"),
        );
        return response;
    }
    let body = match read_json(request).await {
        Ok(body) => body,
        Err(_) => return rejected(400, "invalid json", head),
    };
    if crate::json_fields::fields(&body).is_err() {
        return failure(head);
    }
    let action = match crate::param_changes::text::stripped(body.get("action"), true)
        .map(|value| value.string().unwrap_or_default().to_lowercase())
    {
        Ok(action) => action,
        Err(_) => return failure(head),
    };
    let result: Result<Value, KeyError> = match action.as_str() {
        "remove" => match app.params.lock() {
            Ok(mut params) => clear(&mut params).map_err(KeyError::from),
            Err(_) => return failure(head),
        },
        "add" | "refresh" => {
            let Some(online) = online else {
                return rejected(500, "http client unavailable", head);
            };
            let username = if action == "add" {
                Username::parse(body.get("username"))
            } else {
                match app.params.lock() {
                    Ok(params) => refresh_username(&params),
                    Err(_) => return failure(head),
                }
            };
            let username = match username {
                Ok(username) => username,
                Err(error) => return rejected(error.status(), &error.to_string(), head),
            };
            match tokio::task::spawn_blocking(move || {
                let downloaded = online.fetch(&username)?;
                let mut params = app
                    .params
                    .lock()
                    .map_err(|_| Error::Source("Params lock poisoned".into()))?;
                apply_download(&mut params, &username, &downloaded, timestamp)
            })
            .await
            {
                Ok(result) => result,
                Err(_) => return failure(head),
            }
        }
        _ => return rejected(400, "bad action", head),
    };
    match result {
        Ok(value) => successful(value, head),
        Err(error) => rejected(error.status(), &error.to_string(), head),
    }
}
