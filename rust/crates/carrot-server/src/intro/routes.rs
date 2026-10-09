use super::Intro;
use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub enum Route {
    State,
    Complete,
    Reset,
    ApplyPreset,
}

impl Route {
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "/api/intro/state" => Some(Self::State),
            "/api/intro/complete" => Some(Self::Complete),
            "/api/intro/reset" => Some(Self::Reset),
            "/api/intro/apply_preset" => Some(Self::ApplyPreset),
            _ => None,
        }
    }
}

fn failure(head: bool) -> Response<Body> {
    text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}
fn reply(status: StatusCode, value: Value, head: bool) -> Response<Body> {
    json_response(status, value, head, "").unwrap_or_else(|error| {
        eprintln!("intro response: {error}");
        failure(head)
    })
}
fn rejected(status: StatusCode, message: &str, head: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
        head,
    )
}

fn field(body: &Value, name: &str) -> Result<Value, Error> {
    if !body.truth() {
        return Ok(Value::Null);
    }
    crate::json_fields::fields(body)?;
    Ok(body.get(name).clone())
}

pub async fn handle(
    request: Request<crate::http::RequestBody>,
    app: Arc<Application>,
    intro: Arc<Intro>,
    route: Route,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let allowed = match route {
        Route::State => matches!(request.method(), &Method::GET | &Method::HEAD),
        Route::Complete | Route::Reset | Route::ApplyPreset => request.method() == Method::POST,
    };
    if !allowed {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(match route {
                Route::State => "GET,HEAD",
                Route::Complete | Route::Reset | Route::ApplyPreset => "POST",
            }),
        );
        return response;
    }
    if matches!(route, Route::ApplyPreset)
        && app.params.lock().is_ok_and(|params| !params.has_params())
    {
        return rejected(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Params not available",
            head,
        );
    }
    let body = match route {
        Route::State | Route::Reset => Value::Null,
        Route::Complete => match read_json(request).await {
            Ok(body) => body,
            Err(error) => {
                if let Some(response) = crate::http_response::parser_response(&error, head) {
                    return response;
                }
                Value::Object(Vec::new())
            }
        },
        Route::ApplyPreset => match read_json(request).await {
            Ok(body) => body,
            Err(error) => {
                if let Some(response) = crate::http_response::parser_response(&error, head) {
                    return response;
                }
                return rejected(StatusCode::BAD_REQUEST, "invalid json", head);
            }
        },
    };
    let result = tokio::task::spawn_blocking(move || -> Result<(StatusCode, Value), Error> {
        match route {
            Route::State => {
                let params = app
                    .params
                    .lock()
                    .map_err(|_| Error::Source("Params lock poisoned".into()))?;
                Ok((StatusCode::OK, intro.state_payload(&params)?))
            }
            Route::Complete => {
                let reason = field(&body, "reason")?;
                Ok((
                    StatusCode::OK,
                    Value::object([
                        ("ok", Value::Bool(true)),
                        ("state", intro.mark_completed(&reason)?),
                    ]),
                ))
            }
            Route::Reset => {
                let params = app
                    .params
                    .lock()
                    .map_err(|_| Error::Source("Params lock poisoned".into()))?;
                intro.reset(&params)
            }
            Route::ApplyPreset => {
                let name = field(&body, "preset")?;
                let mut params = app
                    .params
                    .lock()
                    .map_err(|_| Error::Source("Params lock poisoned".into()))?;
                intro.apply_preset(&mut params, &name)
            }
        }
    })
    .await;
    match result {
        Ok(Ok((status, value))) => reply(status, value, head),
        Ok(Err(error)) => {
            eprintln!("intro request: {error}");
            failure(head)
        }
        Err(error) => {
            eprintln!("intro task: {error}");
            failure(head)
        }
    }
}
