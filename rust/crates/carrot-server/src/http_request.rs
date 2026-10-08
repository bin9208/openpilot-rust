use crate::{http::RequestBody, request_body::DecodeFailure, Error, Value};
use http_body_util::BodyExt;
use hyper::Request;

pub async fn read_json(request: Request<RequestBody>) -> Result<Value, Error> {
    read_json_detailed(request)
        .await
        .map_err(|failure| match failure {
            Error::Request(failure @ DecodeFailure::Parser { .. }) => Error::Request(failure),
            Error::Request(failure) => Error::Request(failure.with_message("invalid json".into())),
            _ => Error::Source("invalid json".into()),
        })
}

pub(crate) async fn read_json_detailed(request: Request<RequestBody>) -> Result<Value, Error> {
    let encoding = crate::request_text::encoding(request.headers());
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(Error::Request)?;
        if let Ok(data) = frame.into_data() {
            if bytes.len().saturating_add(data.len()) >= crate::config::BODY_LIMIT {
                return Err(Error::Source("Request Entity Too Large".into()));
            }
            bytes.extend_from_slice(&data);
        }
    }
    let text = crate::request_text::decode(&bytes, &encoding)?;
    Value::parse(&text).map_err(Error::from)
}
