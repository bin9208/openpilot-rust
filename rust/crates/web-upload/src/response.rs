use crate::{compatibility::body_mapping, Error, Fields, Value};

#[derive(Clone, Copy)]
pub enum SessionMode {
    Async,
    Sync,
}
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: ureq::http::HeaderMap,
    pub body: Vec<u8>,
}
impl Response {
    pub fn text(&self) -> Result<String, Error> {
        self.decode(false)
    }
    pub fn text_lossy(&self) -> Result<String, Error> {
        self.decode(true)
    }
    fn decode(&self, replace: bool) -> Result<String, Error> {
        let content_type = self
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        let charset = content_type
            .split(';')
            .skip(1)
            .filter_map(|item| item.trim().split_once('='))
            .find(|(name, _)| name.eq_ignore_ascii_case("charset"))
            .map(|(_, value)| value.trim_matches('"'))
            .unwrap_or("utf-8");
        let encoding =
            encoding_rs::Encoding::for_label(charset.as_bytes()).unwrap_or(encoding_rs::UTF_8);
        if replace {
            return Ok(encoding
                .decode_without_bom_handling(&self.body)
                .0
                .into_owned());
        }
        encoding
            .decode_without_bom_handling_and_without_replacement(&self.body)
            .map(|text| text.into_owned())
            .ok_or(Error::Decode)
    }
    pub(crate) fn mapping(&self) -> Result<(String, Fields), Error> {
        self.mapping_with_mode(false)
    }
    pub(crate) fn mapping_with_mode(&self, replace: bool) -> Result<(String, Fields), Error> {
        let text = self.decode(replace)?;
        let body = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
        Ok((text, body_mapping(body)?))
    }
}
