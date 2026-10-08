use crate::http::Body;
use crate::{Error, Value};
use bytes::Bytes;
use flate2::{
    write::{GzEncoder, ZlibEncoder},
    Compression,
};
use http_body_util::Full;
use hyper::{header, Response, StatusCode};
use std::io::Write;

pub(crate) fn response(
    status: StatusCode,
    bytes: Vec<u8>,
    content_type: &str,
    head: bool,
) -> Response<Body> {
    let length = bytes.len();
    let mut result = Response::new(Full::new(if head {
        Bytes::new()
    } else {
        Bytes::from(bytes)
    }));
    *result.status_mut() = status;
    if let Ok(value) = content_type.parse() {
        result.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if let Ok(value) = length.to_string().parse() {
        result.headers_mut().insert(header::CONTENT_LENGTH, value);
    }
    result
}

pub(crate) fn text(status: StatusCode, text: &str, head: bool) -> Response<Body> {
    response(
        status,
        text.as_bytes().to_vec(),
        "text/plain; charset=utf-8",
        head,
    )
}

pub(crate) fn json_response(
    status: StatusCode,
    payload: Value,
    head: bool,
    coding: &str,
) -> Result<Response<Body>, Error> {
    let mut bytes = payload.encode()?.into_bytes();
    let encoding = if coding.to_lowercase().contains("deflate") {
        "deflate"
    } else if coding.to_lowercase().contains("gzip") {
        "gzip"
    } else {
        ""
    };
    if encoding == "deflate" {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes)?;
        bytes = encoder.finish()?;
    } else if encoding == "gzip" {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes)?;
        bytes = encoder.finish()?;
    }
    let mut result = response(status, bytes, "application/json; charset=utf-8", head);
    if !encoding.is_empty() {
        result.headers_mut().insert(
            header::CONTENT_ENCODING,
            encoding
                .parse()
                .map_err(|_| Error::Source("invalid content encoding".into()))?,
        );
    }
    Ok(result)
}

pub(crate) fn error_response(message: String, head: bool) -> Response<Body> {
    let payload = Value::object([("ok", Value::Bool(false)), ("error", Value::text(&message))]);
    json_response(StatusCode::INTERNAL_SERVER_ERROR, payload, head, "").unwrap_or_else(|_| {
        text(
            StatusCode::INTERNAL_SERVER_ERROR,
            "500: Internal Server Error",
            head,
        )
    })
}
