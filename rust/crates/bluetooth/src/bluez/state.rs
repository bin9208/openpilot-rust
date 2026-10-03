use super::{
    policy::{self, PromptKind},
    Error,
};
use openpilot_logmessaged::JsonValue;
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::oneshot;

#[derive(Clone, Default, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Pair {
    #[default]
    Idle,
    Pairing {
        address: String,
    },
    Paired {
        address: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Cancelled {
        address: String,
    },
    Error {
        address: String,
        error: String,
    },
}

#[derive(Clone, Serialize)]
pub struct Prompt {
    pub id: String,
    pub kind: PromptKind,
    pub value: String,
}
impl Prompt {
    pub(super) fn new(kind: PromptKind, value: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            kind,
            value,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    pub owner: Option<String>,
    pub target: Option<String>,
    pub pair: Pair,
    pub prompt: Option<Prompt>,
    pub answer: Option<oneshot::Sender<JsonValue>>,
}

#[derive(Clone, Default)]
pub(super) struct Shared(Arc<Mutex<State>>);
impl Shared {
    pub fn lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        self.0.lock().map_err(|_| Error::Poisoned)
    }

    pub fn respond(&self, id: &str, value: JsonValue) -> Result<(), Error> {
        let mut state = self.lock()?;
        let prompt = state
            .prompt
            .as_ref()
            .filter(|prompt| prompt.id == id)
            .ok_or(Error::PromptExpired)?;
        if state.answer.as_ref().is_none_or(oneshot::Sender::is_closed) {
            return Err(Error::PromptExpired);
        }
        policy::validate(prompt.kind, &value)?;
        state
            .answer
            .take()
            .ok_or(Error::PromptExpired)?
            .send(value)
            .map_err(|_| Error::PromptExpired)
    }

    pub fn finish(&self) -> Result<(), Error> {
        let mut state = self.lock()?;
        state.target = None;
        state.prompt = None;
        state.answer = None;
        Ok(())
    }
}
