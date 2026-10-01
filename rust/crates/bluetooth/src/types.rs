use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    None,
    AccelCruise,
    DecelCruise,
    GapAdjustCruise,
    LfaButton,
    Cancel,
    AccelCruiseLong,
    DecelCruiseLong,
    GapAdjustCruiseLong,
    LfaButtonLong,
    CancelLong,
    LaneLeft,
    LaneRight,
    PaddleDecel,
    CarrotCruise,
}

impl Action {
    pub(crate) const fn repeats(self) -> bool {
        matches!(
            self,
            Self::AccelCruise | Self::DecelCruise | Self::AccelCruiseLong | Self::DecelCruiseLong
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Token(pub(crate) String);

impl Token {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn with_gesture(&self, gesture: Gesture) -> Self {
        let suffix = match gesture {
            Gesture::Double => "double",
            Gesture::Long => "long",
        };
        Self(format!("{}@{suffix}", self.0))
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Gesture {
    Double,
    Long,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Mapping(pub(crate) IndexMap<Token, Action>);

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Seconds(pub f64);

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum Profile {
    #[default]
    #[serde(rename = "generic")]
    Generic,
    #[serde(rename = "yiser-j6")]
    YiserJ6,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Event {
    pub kind: u16,
    pub code: u16,
    pub value: i32,
    pub at: Seconds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum HoldSource {
    Key(u16),
    Touch,
}
