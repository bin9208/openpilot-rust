use crate::{
    compatibility::text,
    http::{Body, Client, Request},
    Error, Fields, Response, Target, Value,
};
use std::{fs, io::Cursor, path::Path};

pub struct TmuxUpload<'a> {
    pub target: &'a Target,
    pub payload: &'a Fields,
    pub tmux_path: &'a Path,
    pub settings_path: Option<&'a Path>,
}
fn escape(value: &str) -> String {
    value
        .replace('\n', "%0A")
        .replace('\r', "%0D")
        .replace('"', "%22")
}
impl TmuxUpload<'_> {
    pub fn post(&self) -> Result<Response, Error> {
        let boundary = format!("{:032x}", rand::random::<u128>());
        let mut body = Vec::new();
        for (key, value) in self.payload.iter() {
            let values = match value {
                Value::Array(values) => values.clone(),
                Value::Object(values) => values.keys().cloned().map(Value::Text).collect(),
                Value::Null => Vec::new(),
                Value::Bool(_)
                | Value::Integer(_)
                | Value::Float(_)
                | Value::Text(_)
                | Value::PythonText(_) => {
                    vec![value.clone()]
                }
            };
            for value in values {
                if matches!(value, Value::Null) {
                    continue;
                }
                body.extend_from_slice(
                    format!(
                        "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n",
                        escape(key),
                        text(&value)?
                    )
                    .as_bytes(),
                );
            }
        }
        for (field, filename, mime, path) in [
            ("files[0]", "tmux.log", "text/plain", Some(self.tmux_path)),
            (
                "files[1]",
                "toggle_values.json",
                "application/json",
                self.settings_path.filter(|path| path.is_file()),
            ),
        ] {
            if let Some(path) = path {
                let bytes = fs::read(path)?;
                body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
                body.extend_from_slice(&bytes);
                body.extend_from_slice(b"\r\n");
            }
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
        let mut headers = self.target.headers.clone();
        headers.insert(
            "Content-Type".into(),
            format!("multipart/form-data; boundary={boundary}"),
        );
        Client::socket(30).request(
            Request {
                url: &self.target.url,
                method: ureq::http::Method::POST,
                headers,
            },
            Body::Bytes(Cursor::new(body)),
        )
    }
}
