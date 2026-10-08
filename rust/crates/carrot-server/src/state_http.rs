use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
    state_preferences::Preference,
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub(crate) async fn handle(
    request: Request<crate::http::RequestBody>,
    app: Arc<Application>,
    kind: Preference,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let post = request.method() == Method::POST;
    if !matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::POST
    ) {
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
    let payload = if post {
        match read_json(request).await {
            Ok(body) => body,
            Err(error) => {
                if let Some(response) = crate::http_response::parser_response(&error, head) {
                    return response;
                }
                match kind {
                    Preference::Units => {
                        return reject(StatusCode::BAD_REQUEST, "invalid json", head);
                    }
                    Preference::Favorites => Value::Object(Vec::new()),
                }
            }
        }
    } else {
        Value::Object(Vec::new())
    };
    if post && matches!(kind, Preference::Favorites) && !matches!(payload, Value::Object(_)) {
        return reject(StatusCode::BAD_REQUEST, "bad request", head);
    }
    let path = app.config.state.join(kind.path());
    let action = move || {
        if post {
            kind.update(&path, &payload)
        } else {
            kind.read(&path)
        }
    };
    let result = match kind {
        Preference::Units => match tokio::task::spawn_blocking(action).await {
            Ok(result) => result,
            Err(error) => Err(Error::Source(error.to_string())),
        },
        Preference::Favorites => action(),
    };
    match result {
        Ok(Value::Object(fields)) => {
            let mut payload = Value::object([("ok", Value::Bool(true))]);
            if let Value::Object(items) = &mut payload {
                items.extend(fields);
            }
            json_response(StatusCode::OK, payload, head, "").unwrap_or_else(|error| {
                reject(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string(), head)
            })
        }
        Ok(_) => reject(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid preference state",
            head,
        ),
        Err(error) => match kind {
            Preference::Units => {
                reject(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string(), head)
            }
            Preference::Favorites => text(
                StatusCode::INTERNAL_SERVER_ERROR,
                "500 Internal Server Error\n\nServer got itself in trouble",
                head,
            ),
        },
    }
}

fn reject(status: StatusCode, error: &str, head: bool) -> Response<Body> {
    json_response(
        status,
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
