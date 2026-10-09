use super::http::{error, reply};
use crate::{
    http::{Application, Body, RequestBody},
    Error, Value,
};
use hyper::{Method, Request, Response, StatusCode};
use std::sync::Arc;

pub async fn handle(
    request: Request<RequestBody>,
    app: Arc<Application>,
    path: &str,
) -> Response<Body> {
    let head = request.method() == Method::HEAD;
    let force = url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
        .find(|(key, _)| key == "force")
        .is_some_and(|(_, value)| value == "1");
    if path == "/api/device_network" && !force {
        let value = (|| {
            let params = app
                .params
                .lock()
                .map_err(|_| Error::Source("Params lock poisoned".into()))?;
            app.system.network.snapshot(&params)
        })();
        return match value {
            Ok(value) => reply(
                StatusCode::OK,
                Value::object([("ok", Value::Bool(true)), ("network", value)]),
                head,
            ),
            Err(failure) => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &failure.to_string(),
                head,
            ),
        };
    }
    let path = path.to_owned();
    let result = tokio::task::spawn_blocking(move || -> Result<_, Error> {
        let (key, value) = match path.as_str() {
            "/api/device_network" => {
                let probe = app.system.network.probe();
                let params = app
                    .params
                    .lock()
                    .map_err(|_| Error::Source("Params lock poisoned".into()))?;
                ("network", app.system.network.publish(probe, &params)?)
            }
            "/api/calibration_status" => {
                let params = app
                    .params
                    .lock()
                    .map_err(|_| Error::Source("Params lock poisoned".into()))?
                    .native_params()
                    .cloned();
                ("calibration", super::calibration::status(params.as_ref())?)
            }
            "/api/regulatory" => {
                let path = &app.system.regulatory;
                if !path.is_file() {
                    return Ok(None);
                }
                let bytes =
                    std::fs::read(path).map_err(|error| crate::state::io_error(error, path))?;
                let html = String::from_utf8_lossy(&bytes)
                    .replace("\r\n", "\n")
                    .replace('\r', "\n");
                ("html", Value::text(&html))
            }
            _ => return Err(Error::Source("unregistered system read".into())),
        };
        Ok(Some(Value::object([
            ("ok", Value::Bool(true)),
            (key, value),
        ])))
    })
    .await;
    match result {
        Ok(Ok(Some(value))) => reply(StatusCode::OK, value, head),
        Ok(Ok(None)) => error(StatusCode::NOT_FOUND, "regulatory info unavailable", head),
        Ok(Err(failure)) => error(
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
