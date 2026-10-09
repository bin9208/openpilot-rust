use crate::{video::ipc::Camera, Error};
use openpilot_logmessaged::{JsonValue, JsonView};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ClientKey(Vec<u32>);

pub(crate) struct StreamRequest {
    pub sdp: String,
    pub cameras: Vec<Camera>,
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
    pub client: Vec<u32>,
    pub device: Vec<u32>,
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

fn whitespace(value: u32) -> bool {
    char::from_u32(value)
        .is_some_and(|value| value.is_whitespace() || matches!(value, '\u{1c}'..='\u{1f}'))
}

fn strip(points: &[u32]) -> &[u32] {
    let start = points
        .iter()
        .position(|point| !whitespace(*point))
        .unwrap_or(points.len());
    let end = points
        .iter()
        .rposition(|point| !whitespace(*point))
        .map_or(start, |index| index + 1);
    &points[start..end]
}

fn normalize(value: Option<JsonValue>) -> Result<Vec<u32>, Error> {
    let Some(value) = value.filter(truth) else {
        return Ok(Vec::new());
    };
    let source = openpilot_runtime_version::python_str(&value)?;
    let mut result = String::new();
    let mut bad = false;
    for point in strip(&source) {
        if let Some(value) = char::from_u32(*point)
            .filter(|value| value.is_ascii_alphanumeric() || matches!(value, '.' | '_' | ':' | '-'))
        {
            result.push(value);
            bad = false;
        } else if !bad {
            result.push('-');
            bad = true;
        }
    }
    Ok(result
        .trim_matches('-')
        .chars()
        .take(128)
        .map(u32::from)
        .collect())
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
            input.get("client_id").map_or(Ok(Vec::new()), |value| {
                openpilot_runtime_version::python_str(&value)
            })?
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
                Vec::new()
            },
            takeover: input.get("takeover").is_some_and(|value| truth(&value)),
            carrot_state: input.get("carrot_state").is_some_and(|value| truth(&value)),
        })
    }

    pub fn key(&self, remote: &str) -> ClientKey {
        let client = if self.device.is_empty() {
            &self.client
        } else {
            &self.device
        };
        let normalized: Vec<_> = strip(client).iter().take(128).copied().collect();
        if normalized.is_empty() {
            ClientKey(format!("remote:{remote}").chars().map(u32::from).collect())
        } else {
            ClientKey("client:".chars().map(u32::from).chain(normalized).collect())
        }
    }
}
