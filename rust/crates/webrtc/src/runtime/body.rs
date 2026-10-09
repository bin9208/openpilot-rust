//! Original aiohttp request text decoding, before Python-compatible JSON parsing.
use super::http::{internal, reply, Reply};
use crate::Error;
use charset_norm::codecs::{decode, Errors};
use http_body_util::BodyExt;
use hyper::{body::Incoming, header, Request, StatusCode};

fn charset(content_type: &str) -> &str {
    content_type
        .split(';')
        .skip(1)
        .find_map(|parameter| {
            let (key, value) = parameter.split_once('=').unwrap_or((parameter, ""));
            key.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| value.trim_matches([' ', '"']))
        })
        .filter(|value| !value.is_empty())
        .unwrap_or("utf-8")
}

pub(super) async fn read(request: Request<Incoming>) -> Result<String, Reply> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .map_or(Ok(""), |value| value.to_str())
        .map_err(|_| internal(&Error::Contract("invalid request charset header")))?
        .to_owned();
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame
            .map_err(|error| reply(StatusCode::BAD_REQUEST, error.to_string(), false, false))?;
        if let Ok(data) = frame.into_data() {
            bytes.extend_from_slice(&data);
            if bytes.len() >= 1_048_576 {
                return Err(reply(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    format!(
                        "Maximum request body size 1048576 exceeded, actual body size {}",
                        bytes.len()
                    ),
                    false,
                    false,
                ));
            }
        }
    }
    decode(&bytes, charset(&content_type), Errors::Strict)
        .map_err(|_| internal(&Error::Contract("request charset decoding failed")))
}
