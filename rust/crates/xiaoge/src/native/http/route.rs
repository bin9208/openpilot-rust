use super::{body, request::Received, Error, Response};
use crate::{native::shared::Shared, service::Stream};
use std::{fs, path::Path};

pub fn respond(request: Received, shared: &Shared, index: &Path) -> Result<Response, Error> {
    let request = match request {
        Received::Request(request) => request,
        Received::Response(response) => return Ok(response),
    };
    let url = super::target::Target::parse(&request.path);
    match request.method.as_str() {
        "GET" => match url.path {
            "/" => Ok(Response {
                status: 200,
                body: fs::read(index)?,
                content_type: "text/html; charset=utf-8",
                cache: true,
            }),
            "/api/status" => Ok(Response::json(200, shared.status()?)),
            "/api/config" => Ok(Response::json(
                200,
                serde_json::to_vec(&shared.state()?.config)?,
            )),
            "/api/snapshot" => {
                let stream = url::form_urlencoded::parse(url.query.as_bytes())
                    .find(|(key, value)| key == "stream" && !value.is_empty())
                    .map(|(_, value)| value.into_owned())
                    .unwrap_or_else(|| "wide".to_owned());
                let stream = match stream.as_str() {
                    "wide" => Stream::Wide,
                    "road" => Stream::Road,
                    _ => return Response::message(400, "stream must be wide or road"),
                };
                match shared.snapshot(stream)? {
                    Some(jpeg) => Ok(Response {
                        status: 200,
                        body: jpeg.to_vec(),
                        content_type: "image/jpeg",
                        cache: false,
                    }),
                    None => Response::message(
                        503,
                        &format!("no {} camera frame available", stream.name()),
                    ),
                }
            }
            _ => Ok(Response::error(404)),
        },
        "POST" => {
            let value = match body::json(&request.body) {
                Ok(value) => value,
                Err(message) => return Response::message(400, &message),
            };
            let result = match url.path {
                "/api/config" => shared
                    .save_config(&value)
                    .and_then(|config| Ok(Response::json(200, serde_json::to_vec(&config)?))),
                "/api/settings" => shared
                    .set_settings(&value)
                    .and_then(|()| Ok(Response::json(200, shared.status()?))),
                _ => return Ok(Response::error(404)),
            };
            match result {
                Err(Error::Policy(crate::Error::Invalid(message))) => {
                    Response::message(400, message)
                }
                result => result,
            }
        }
        "DELETE" => {
            if url.path != "/api/config" {
                return Ok(Response::error(404));
            }
            shared.clear_config()?;
            Ok(Response::json(200, b"{\"success\": true}".to_vec()))
        }
        _ => Ok(Response::error(501)),
    }
}
