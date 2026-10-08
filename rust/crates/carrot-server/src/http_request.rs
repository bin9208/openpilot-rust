use crate::{Error, Value};
use http_body_util::BodyExt;
use hyper::{body::Incoming, Request};

pub async fn read_json(request: Request<Incoming>) -> Result<Value, Error> {
    let mut body = request.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| Error::Source("invalid json".into()))?;
        if let Ok(data) = frame.into_data() {
            if bytes.len().saturating_add(data.len()) >= crate::config::BODY_LIMIT {
                return Err(Error::Source("invalid json".into()));
            }
            bytes.extend_from_slice(&data);
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| Error::Source("invalid json".into()))?;
    Value::parse(text).map_err(|_| Error::Source("invalid json".into()))
}
