use crate::{
    http::{Application, Body, RequestBody},
    http_response::{json_response, text},
    Error, Value,
};
use hyper::{header, Method, Request, Response, StatusCode};
use std::sync::Arc;

pub fn matches(path: &str) -> bool {
    matches!(
        path,
        "/api/reboot"
            | "/api/poweroff"
            | "/api/recalibrate"
            | "/api/set_default"
            | "/api/time_sync"
            | "/api/device_network"
            | "/api/calibration_status"
            | "/api/regulatory"
    )
}

pub(super) fn reply(status: StatusCode, body: Value, head: bool) -> Response<Body> {
    json_response(status, body, head, "").unwrap_or_else(|_| internal(head))
}

pub(super) fn error(status: StatusCode, message: &str, head: bool) -> Response<Body> {
    reply(
        status,
        Value::object([("ok", Value::Bool(false)), ("error", Value::text(message))]),
        head,
    )
}

pub(super) fn internal(head: bool) -> Response<Body> {
    let mut response = text(
        StatusCode::INTERNAL_SERVER_ERROR,
        "500 Internal Server Error\n\nServer got itself in trouble",
        head,
    );
    response.headers_mut().insert(
        header::CONNECTION,
        header::HeaderValue::from_static("close"),
    );
    response
        .extensions_mut()
        .insert(hyper::ext::CloseAfterResponse);
    response
}

fn write(app: &Application, path: &str) -> Result<Value, super::actions::Failure> {
    if path == "/api/set_default" {
        let mut params = app
            .params
            .lock()
            .map_err(|_| Error::Source("Params lock poisoned".into()))?;
        if !params.has_params() {
            return Err(Error::Source("params unavailable".into()).into());
        }
        let catalog = app.catalog(&params)?;
        return Ok(super::defaults::reset(&mut params, &catalog, &app.history)?);
    }
    let action = match path {
        "/api/reboot" => super::actions::Action::Reboot,
        "/api/poweroff" => super::actions::Action::Poweroff,
        "/api/recalibrate" => super::actions::Action::Recalibrate,
        _ => return Err(Error::Source("unregistered system action".into()).into()),
    };
    let engaged = app.drive_engaged();
    let mut params = app
        .params
        .lock()
        .map_err(|_| Error::Source("Params lock poisoned".into()))?;
    super::actions::run(action, &mut params, engaged)
}

pub async fn handle(
    request: Request<RequestBody>,
    app: Arc<Application>,
    path: String,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let read = matches!(
        path.as_str(),
        "/api/device_network" | "/api/calibration_status" | "/api/regulatory"
    );
    if (read && !matches!(request.method(), &Method::GET | &Method::HEAD))
        || (!read && request.method() != Method::POST)
    {
        let mut response = text(
            StatusCode::METHOD_NOT_ALLOWED,
            "405: Method Not Allowed",
            head,
        );
        response.headers_mut().insert(
            header::ALLOW,
            header::HeaderValue::from_static(if read { "GET,HEAD" } else { "POST" }),
        );
        return response;
    }
    if read {
        return super::http_read::handle(request, app, &path).await;
    }
    if path == "/api/time_sync" {
        return super::http_time::handle(request, app).await;
    }
    match tokio::task::spawn_blocking(move || write(&app, &path)).await {
        Ok(Ok(body)) => reply(
            if body.get("ok").truth() {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            body,
            head,
        ),
        Ok(Err(super::actions::Failure::Engaged)) => {
            error(StatusCode::CONFLICT, "Disengage first", head)
        }
        Ok(Err(super::actions::Failure::Boundary(failure))) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &failure.to_string(),
            head,
        ),
        Err(failure) => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &failure.to_string(),
            head,
        ),
    }
}
