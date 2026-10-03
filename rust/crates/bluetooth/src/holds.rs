use crate::types::{HoldSource, Seconds, Token};
use indexmap::IndexMap;

pub(crate) struct Hold {
    pub token: Token,
    pub started: Seconds,
    pub last: Option<Seconds>,
    pub expired: bool,
}

pub(crate) fn update(
    holds: &mut IndexMap<HoldSource, Hold>,
    source: HoldSource,
    token: Option<Token>,
    started: Seconds,
) {
    if let Some(previous) = holds.get_mut(&source) {
        if Some(&previous.token) != token.as_ref() {
            if previous.last.is_some() || previous.expired {
                previous.expired = true;
                return;
            }
            holds.shift_remove(&source);
        }
    }
    if !holds.contains_key(&source) {
        if let Some(token) = token {
            holds.insert(
                source,
                Hold {
                    token,
                    started,
                    last: None,
                    expired: false,
                },
            );
        }
    }
}
