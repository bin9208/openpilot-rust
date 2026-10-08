//! Original params_restore first-part boundary; raw multipart data is never decoded here.
mod disposition;

use crate::{http::RequestBody, Error, Value};
use http_body_util::BodyExt;
use hyper::{header, Request};
use multer::{Field, Multipart};

pub enum FirstFile {
    MissingFileField,
    Data(Vec<u8>),
}

struct Mime {
    kind: String,
    subtype: String,
    boundary: Option<String>,
}

impl Mime {
    fn parse(value: &str) -> Self {
        let mut parts = value.split(';');
        let mime = parts.next().unwrap_or("").trim().to_lowercase();
        let (kind, subtype) = mime.split_once('/').unwrap_or((&mime, ""));
        let subtype = subtype.split_once('+').map_or(subtype, |(name, _)| name);
        let boundary = parts.find_map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            key.to_lowercase()
                .trim()
                .eq("boundary")
                .then(|| value.trim_matches([' ', '"']).to_owned())
        });
        Self {
            kind: kind.to_owned(),
            subtype: subtype.to_owned(),
            boundary,
        }
    }

    fn boundary(&self, content_type: &str) -> Result<&str, Error> {
        if self.kind != "multipart" {
            return Err(Error::Source("multipart/* content type expected".into()));
        }
        let boundary = self.boundary.as_deref().ok_or_else(|| {
            Error::Source(format!("boundary missed for Content-Type: {content_type}"))
        })?;
        if boundary.chars().count() > 70 {
            return Err(Error::Source(format!(
                "boundary {} is too long (70 chars max)",
                Value::text(boundary).repr()?
            )));
        }
        Ok(boundary)
    }
}

fn nested(field: &Field<'_>) -> Result<bool, Error> {
    let Some(value) = field.headers().get(header::CONTENT_TYPE) else {
        return Ok(false);
    };
    let value = String::from_utf8_lossy(value.as_bytes());
    let mime = Mime::parse(&value);
    if mime.kind != "multipart" {
        return Ok(false);
    }
    mime.boundary(&value)?;
    Ok(true)
}

fn field_error(error: &multer::Error) -> Error {
    match error {
        multer::Error::StreamReadFailed(error) => {
            if let Some(failure) = error.downcast_ref::<crate::DecodeFailure>() {
                return Error::Request(failure.clone());
            }
            Error::Source("failed to read stream".into())
        }
        multer::Error::IncompleteFieldData { .. } => Error::Source("Reading after EOF".into()),
        multer::Error::HeaderTooLong { .. } => Error::Source(format!("400, message:\n  {error}")),
        _ => Error::Source(error.to_string()),
    }
}

pub async fn first_file(request: Request<RequestBody>) -> Result<FirstFile, Error> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .ok_or_else(|| Error::Source("'Content-Type'".into()))?;
    let content_type = String::from_utf8_lossy(content_type.as_bytes()).into_owned();
    let mime = Mime::parse(&content_type);
    let boundary = mime.boundary(&content_type)?;
    let body = request.into_body();
    let mut reader = Multipart::new(body.into_data_stream(), boundary);
    let mut field = match reader.next_field().await {
        Ok(Some(field)) => field,
        Ok(None) => return Ok(FirstFile::MissingFileField),
        Err(multer::Error::IncompleteStream) => {
            return Err(Error::Source(format!(
                "Could not find starting boundary {}",
                disposition::bytes_repr(format!("--{boundary}").as_bytes())?
            )));
        }
        Err(error) => return Err(field_error(&error)),
    };
    if nested(&field)? {
        return Err(Error::Source(
            "'MultipartReader' object has no attribute 'name'".into(),
        ));
    }
    let name = disposition::name(
        field
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .map(header::HeaderValue::as_bytes),
    )?;
    if mime.subtype == "form-data" && name.as_deref() == Some("_charset_") {
        let mut bytes = Vec::new();
        while bytes.len() < 32 {
            let Some(chunk) = field.chunk().await.map_err(|error| field_error(&error))? else {
                break;
            };
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() > 31 {
            return Err(Error::Source("Invalid default charset".into()));
        }
        let mut charset = bytes.as_slice();
        while let Some((first, rest)) = charset.split_first() {
            if !b" \t\r\n\x0b\x0c".contains(first) {
                break;
            }
            charset = rest;
        }
        while let Some((last, rest)) = charset.split_last() {
            if !b" \t\r\n\x0b\x0c".contains(last) {
                break;
            }
            charset = rest;
        }
        crate::request_text::decode(charset, "utf-8")?;
        drop(field);
        let ending = if reader
            .next_field()
            .await
            .map_err(|error| field_error(&error))?
            .is_none()
        {
            "--"
        } else {
            ""
        };
        return Err(Error::Source(format!(
            "400, message:\n  Invalid HTTP header: {}",
            disposition::bytes_repr(format!("--{boundary}{ending}").as_bytes())?
        )));
    }
    if name.as_deref() != Some("file") {
        return Ok(FirstFile::MissingFileField);
    }
    let length = if mime.subtype != "form-data" {
        field
            .headers()
            .get(header::CONTENT_LENGTH)
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
    } else {
        None
    };
    let bytes = field.bytes().await.map_err(|error| field_error(&error))?;
    if let Some(length) = length {
        let expected = Value::text(&length).int()?;
        if expected != Value::integer(bytes.len()).int()? {
            return Err(Error::Source(
                "Reader did not read all the data or it is malformed".into(),
            ));
        }
    }
    Ok(FirstFile::Data(bytes.to_vec()))
}
