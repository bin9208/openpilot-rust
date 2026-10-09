use super::{
    clear, complete_validation, get, prepare_validation, reject, set, Online, Reply, Validation,
};
use crate::{
    http::{read_json, Application, Body},
    http_response::{json_response, text},
};
use hyper::{header, Method, Request, Response};
use std::sync::Arc;

pub fn matches(path: &str, method: &Method) -> bool {
    path == "/api/mapbox/tokens"
        || (matches!(path, "/api/mapbox/token" | "/api/mapbox/token/validate")
            && !matches!(method, &Method::GET | &Method::HEAD))
}

fn failure(head: bool) -> Response<Body> {
    text(
        hyper::StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    )
}

fn reply(reply: Reply, head: bool) -> Response<Body> {
    let status = hyper::StatusCode::from_u16(reply.status)
        .unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR);
    json_response(status, reply.body, head, "").unwrap_or_else(|_| failure(head))
}

pub async fn handle(
    request: Request<crate::http::RequestBody>,
    app: Arc<Application>,
    online: Arc<Online>,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let path = percent_encoding::percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .into_owned();
    let allowed = if path.ends_with("/tokens") {
        "GET,HEAD"
    } else if path.ends_with("/validate") {
        "GET,HEAD,POST"
    } else {
        "DELETE,GET,HEAD,POST"
    };
    let method = request.method().clone();
    let valid = if path.ends_with("/tokens") {
        matches!(method, Method::GET | Method::HEAD)
    } else if path.ends_with("/validate") {
        method == Method::POST
    } else {
        matches!(method, Method::POST | Method::DELETE)
    };
    if !valid {
        let mut response = text(
            hyper::StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static(allowed));
        return response;
    }
    let result = if path.ends_with("/tokens") {
        let params = match app.params.lock() {
            Ok(params) => params,
            Err(_) => return failure(head),
        };
        get(&params)
    } else if method == Method::DELETE {
        let mut query = crate::Value::Object(Vec::new());
        for (key, value) in
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        {
            if !query.has(&key)
                && crate::json_fields::set(&mut query, &key, crate::Value::text(&value)).is_err()
            {
                return failure(head);
            }
        }
        let mut params = match app.params.lock() {
            Ok(params) => params,
            Err(_) => return failure(head),
        };
        clear(&query, &mut params)
    } else {
        let validate = path.ends_with("/validate");
        let can_read = request
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|length| length != "0")
            || request.headers().contains_key(header::TRANSFER_ENCODING);
        let body = if validate && !can_read {
            crate::Value::Object(Vec::new())
        } else {
            match read_json(request).await {
                Ok(body) => body,
                Err(error) => {
                    if let Some(response) = crate::http_response::parser_response(&error, head) {
                        return response;
                    }
                    return reply(reject(400, "invalid json"), head);
                }
            }
        };
        if validate {
            let prepared = {
                let params = match app.params.lock() {
                    Ok(params) => params,
                    Err(_) => return failure(head),
                };
                prepare_validation(&body, &params)
            };
            match prepared {
                Ok(Validation::Complete(reply)) => Ok(reply),
                Ok(Validation::Public { token, result }) => {
                    match tokio::task::spawn_blocking(move || {
                        online
                            .validate(&token)
                            .and_then(|online| complete_validation(result, online))
                    })
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => return failure(head),
                    }
                }
                Err(error) => Err(error),
            }
        } else {
            let mut params = match app.params.lock() {
                Ok(params) => params,
                Err(_) => return failure(head),
            };
            set(&body, &mut params)
        }
    };
    match result {
        Ok(result) => reply(result, head),
        Err(_) => failure(head),
    }
}
