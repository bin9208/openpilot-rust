use crate::{video::ipc::Camera, Error};
use openpilot_logmessaged::{JsonValue, JsonView};

pub(crate) struct StreamRequest {
    pub sdp: String,
    pub cameras: Vec<Camera>,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
    pub client: String,
    pub device: String,
    pub takeover: bool,
    pub carrot_state: bool,
}

pub(crate) fn truth(value: &JsonValue) -> bool {
    match value.view() {
        JsonView::Null => false,
        JsonView::Bool(value) => value,
        JsonView::Integer(value) => value != "0",
        JsonView::Float(value) => value != 0.0,
        JsonView::Text(value) => !value.is_empty(),
        JsonView::Array(value) => !value.is_empty(),
        JsonView::Object(value) => !value.is_empty(),
    }
}

fn source_str(value: &JsonValue) -> Result<String, Error> {
    match value.view() {
        JsonView::Null => Ok("None".to_owned()),
        JsonView::Bool(value) => Ok(if value { "True" } else { "False" }.to_owned()),
        JsonView::Text(_) => value
            .to_utf8()
            .ok_or(Error::Contract("identifier contains non-UTF8 Unicode")),
        JsonView::Integer(value) => Ok(value.to_owned()),
        JsonView::Float(value) => {
            if value.is_nan() {
                return Ok("nan".to_owned());
            }
            if value.is_infinite() {
                return Ok(if value.is_sign_negative() {
                    "-inf"
                } else {
                    "inf"
                }
                .to_owned());
            }
            let mut output = String::new();
            openpilot_runtime_core::python_float::write_float(value, &mut output)?;
            Ok(output)
        }
        JsonView::Array(_) | JsonView::Object(_) => {
            Err(Error::Contract("identifier must be scalar"))
        }
    }
}

fn strings(value: Option<JsonValue>) -> Result<Vec<String>, Error> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let JsonView::Array(values) = value.view() else {
        return Err(Error::Contract("expected string list"));
    };
    values
        .iter()
        .map(|value| {
            value
                .to_utf8()
                .ok_or(Error::Contract("expected UTF-8 string"))
        })
        .collect()
}

fn whitespace(value: char) -> bool {
    value.is_whitespace() || matches!(value, '\u{1c}'..='\u{1f}')
}

fn normalize(value: Option<JsonValue>) -> Result<String, Error> {
    let Some(value) = value.filter(truth) else {
        return Ok(String::new());
    };
    let source = source_str(&value)?;
    let mut result = String::new();
    let mut bad = false;
    for value in source.trim_matches(whitespace).chars() {
        if value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | ':' | '-') {
            result.push(value);
            bad = false;
        } else if !bad {
            result.push('-');
            bad = true;
        }
    }
    Ok(result.trim_matches('-').chars().take(128).collect())
}

impl StreamRequest {
    pub fn parse(source: &str, carrot: bool) -> Result<Self, Error> {
        let input = JsonValue::parse(source)?;
        let sdp = input
            .get("sdp")
            .and_then(|value| value.to_utf8())
            .ok_or(Error::Contract("missing SDP"))?;
        let cameras = strings(Some(
            input
                .get("cameras")
                .ok_or(Error::Contract("missing cameras"))?,
        ))?
        .iter()
        .map(|camera| Camera::parse(camera))
        .collect::<Result<Vec<_>, _>>()?;
        let client = if carrot {
            normalize(input.get("client_id"))?
        } else {
            input
                .get("client_id")
                .map_or(Ok(String::new()), |value| source_str(&value))?
        };
        Ok(Self {
            sdp,
            cameras,
            incoming: strings(input.get("bridge_services_in"))?,
            outgoing: strings(input.get("bridge_services_out"))?,
            client,
            device: if carrot {
                normalize(input.get("device_id"))?
            } else {
                String::new()
            },
            takeover: input.get("takeover").is_some_and(|value| truth(&value)),
            carrot_state: input.get("carrot_state").is_some_and(|value| truth(&value)),
        })
    }

    pub fn key(&self, remote: &str) -> String {
        let client = if self.device.is_empty() {
            &self.client
        } else {
            &self.device
        };
        let normalized: String = client.trim_matches(whitespace).chars().take(128).collect();
        if normalized.is_empty() {
            format!("remote:{remote}")
        } else {
            format!("client:{normalized}")
        }
    }
}
