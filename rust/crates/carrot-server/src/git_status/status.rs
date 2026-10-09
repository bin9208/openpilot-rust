use num_traits::ToPrimitive;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Error,
    Busy,
    NoUpstream,
    Ok,
    FetchError,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub available: bool,
    pub state: State,
    pub behind: u64,
    pub ahead: u64,
    pub branch: String,
    pub upstream: String,
    pub checked_at: i64,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch_error: Option<String>,
}

impl Status {
    pub(super) fn error(message: impl Into<String>, now: f64) -> Result<Self, super::Failure> {
        Ok(Self {
            available: false,
            state: State::Error,
            behind: 0,
            ahead: 0,
            branch: String::new(),
            upstream: String::new(),
            checked_at: now.trunc().to_i64().ok_or(super::Failure::Clock)?,
            error: message.into(),
            head: None,
            target_head: None,
            remote: None,
            remote_branch: None,
            fetch_error: None,
        })
    }
}
