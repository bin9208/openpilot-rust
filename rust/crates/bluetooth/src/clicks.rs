use crate::types::{Action, Gesture, Mapping, Seconds, Token};
use indexmap::IndexMap;

pub(crate) struct Clicks {
    pub mapping: Mapping,
    learning: bool,
    pub pending: IndexMap<Token, Seconds>,
}

impl Clicks {
    pub fn new(mapping: Mapping, learning: bool) -> Self {
        Self {
            mapping,
            learning,
            pending: IndexMap::new(),
        }
    }

    pub fn assigned(&self, token: &Token, gesture: Gesture) -> bool {
        self.learning
            || self
                .mapping
                .0
                .get(&token.with_gesture(gesture))
                .is_some_and(|action| *action != Action::None)
    }

    pub fn flush(&mut self, now: Seconds) -> Vec<Token> {
        let mut result = Vec::new();
        self.pending.retain(|token, released| {
            let age = now.0 - released.0;
            if age >= 0.35 {
                if age <= 0.4 {
                    result.push(token.clone());
                }
                false
            } else {
                true
            }
        });
        result
    }

    pub fn release(&mut self, token: Token, duration: Seconds, now: Seconds) -> Vec<Token> {
        let mut result = self.flush(now);
        if !(0.0..=10.0).contains(&duration.0) {
            return result;
        }
        if duration.0 >= 0.7 && self.assigned(&token, Gesture::Long) {
            if self.pending.shift_remove(&token).is_some() {
                result.push(token.clone());
            }
            result.push(token.with_gesture(Gesture::Long));
        } else if self.assigned(&token, Gesture::Double) {
            if self.pending.shift_remove(&token).is_some() {
                result.push(token.with_gesture(Gesture::Double));
            } else {
                self.pending.insert(token, now);
            }
        } else {
            result.push(token);
        }
        result
    }
}
