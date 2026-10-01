use super::digits::{DECIMAL, DIGITS};
use openpilot_logmessaged::{JsonValue, JsonView};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum PromptKind {
    RequestConfirmation,
    RequestAuthorization,
    AuthorizeService,
    RequestPinCode,
    RequestPasskey,
    DisplayPinCode,
    DisplayPasskey,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("confirmation must be boolean")]
    Confirmation,
    #[error("passkey must contain 1 to 6 digits")]
    Passkey,
    #[error("PIN must contain 1 to 16 characters")]
    Pin,
    #[error(transparent)]
    Text(#[from] openpilot_runtime_version::Error),
}

pub fn digit(point: u32) -> bool {
    let index = DIGITS.partition_point(|(start, _)| *start <= point);
    index
        .checked_sub(1)
        .and_then(|index| DIGITS.get(index))
        .is_some_and(|(_, end)| point <= *end)
}

pub fn decimal(point: u32) -> Option<u32> {
    let index = DECIMAL.partition_point(|start| *start <= point);
    let start = DECIMAL.get(index.checked_sub(1)?)?;
    let value = point.checked_sub(*start)?;
    (value < 10).then_some(value)
}

pub fn validate(kind: PromptKind, value: &JsonValue) -> Result<(), Error> {
    if matches!(value.view(), JsonView::Bool(false)) {
        return Ok(());
    }
    match kind {
        PromptKind::RequestConfirmation
        | PromptKind::RequestAuthorization
        | PromptKind::AuthorizeService => {
            if matches!(value.view(), JsonView::Bool(true)) {
                Ok(())
            } else {
                Err(Error::Confirmation)
            }
        }
        PromptKind::RequestPinCode => match value.view() {
            JsonView::Text(points) if (1..=16).contains(&points.len()) => Ok(()),
            _ => Err(Error::Pin),
        },
        PromptKind::RequestPasskey => {
            let points = openpilot_runtime_version::python_str(value)?;
            if (1..=6).contains(&points.len()) && points.into_iter().all(digit) {
                Ok(())
            } else {
                Err(Error::Passkey)
            }
        }
        PromptKind::DisplayPinCode | PromptKind::DisplayPasskey => Ok(()),
    }
}
