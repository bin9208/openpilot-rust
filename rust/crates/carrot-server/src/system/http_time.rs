use super::{
    http::{error, internal, reply},
    time_sync,
};
use crate::{
    http::{Application, Body, RequestBody},
    http_request::read_json_detailed,
    Error,
};
use hyper::{Request, Response, StatusCode};
use std::sync::Arc;

pub async fn handle(request: Request<RequestBody>, app: Arc<Application>) -> Response<Body> {
    let body = match read_json_detailed(request).await {
        Ok(body) => body,
        Err(failure) => {
            if let Some(response) = crate::http_response::parser_response(&failure, false) {
                return response;
            }
            return error(
                StatusCode::BAD_REQUEST,
                &format!("bad json: {failure}"),
                false,
            );
        }
    };
    let request = match time_sync::Request::parse(&body) {
        Ok(request) => request,
        Err(Error::Source(message)) if message == "epoch_ms required" => {
            return error(StatusCode::BAD_REQUEST, &message, false)
        }
        Err(_) => return internal(false),
    };
    match tokio::task::spawn_blocking(move || app.system.time_sync.sync(&request)).await {
        Ok(Ok(value)) => reply(
            if value.get("ok").truth() {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            value,
            false,
        ),
        Ok(Err(_)) | Err(_) => internal(false),
    }
}
