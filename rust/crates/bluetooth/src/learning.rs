use crate::{Address, Seconds};
use openpilot_logmessaged::{JsonValue, JsonView};

#[derive(Default)]
pub(crate) struct Learning(Option<JsonValue>);

impl Learning {
    pub fn new(value: Option<JsonValue>, now: Seconds) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        if !value.is_object() {
            return Self::default();
        }
        let until = value
            .get("until")
            .map_or(Some(0.0), |until| match until.view() {
                JsonView::Bool(value) => Some(f64::from(u8::from(value))),
                JsonView::Integer(value) => value.parse().ok(),
                JsonView::Float(value) => Some(value),
                _ => None,
            });
        if until.is_none_or(|until| until < now.0) {
            Self::default()
        } else {
            Self(Some(value))
        }
    }

    pub fn matches(&self, address: &Address) -> bool {
        self.0
            .as_ref()
            .and_then(|value| value.get("address"))
            .is_some_and(|value| value.text_eq(address.as_str()))
    }

    pub fn json(&self) -> Result<String, std::fmt::Error> {
        self.0
            .as_ref()
            .map_or_else(|| Ok("{}".to_owned()), JsonValue::to_json)
    }
}
