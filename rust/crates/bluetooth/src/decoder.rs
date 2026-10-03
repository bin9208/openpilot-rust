use crate::types::{HoldSource, Mapping, Profile, Seconds, Token};
use crate::{clicks::Clicks, holds::Hold, types::Gesture};
use indexmap::IndexMap;
use std::collections::BTreeSet;

pub(crate) struct Point {
    pub x: i32,
    pub y: i32,
}

pub(crate) struct Touch {
    pub start: Point,
    pub last: Point,
}

pub struct Decoder {
    pub(crate) profile: Profile,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) touch: bool,
    pub(crate) track: Option<Touch>,
    pub(crate) started: Seconds,
    pub(crate) down: IndexMap<u16, Seconds>,
    pub(crate) pending_keys: Vec<(u16, Seconds)>,
    pub(crate) dropped: bool,
    pub(crate) clicks: Clicks,
    pub(crate) holds: IndexMap<HoldSource, Hold>,
    pub(crate) frame_open: bool,
    pub(crate) repeated: BTreeSet<Token>,
}

impl Decoder {
    pub fn new(profile: Profile, mapping: Mapping, learning: bool) -> Self {
        Self {
            profile,
            x: 0,
            y: 0,
            touch: false,
            track: None,
            started: Seconds(0.0),
            down: IndexMap::new(),
            pending_keys: Vec::new(),
            dropped: false,
            clicks: Clicks::new(mapping, learning),
            holds: IndexMap::new(),
            frame_open: false,
            repeated: BTreeSet::new(),
        }
    }

    pub fn active_longs(&self) -> BTreeSet<Token> {
        self.holds
            .values()
            .filter(|hold| hold.last.is_some() && !hold.expired)
            .map(|hold| hold.token.with_gesture(Gesture::Long))
            .collect()
    }

    pub fn repeated(&self) -> &BTreeSet<Token> {
        &self.repeated
    }

    pub fn cancel_holds(&mut self) {
        for hold in self.holds.values_mut() {
            hold.expired = true;
        }
    }

    pub fn flush(&mut self, now: Seconds) -> Vec<Token> {
        self.repeated.clear();
        if self.dropped || self.frame_open {
            return Vec::new();
        }
        let mut result = self.clicks.flush(now);
        for (source, hold) in &mut self.holds {
            let duration = now.0 - hold.started.0;
            let age = match source {
                HoldSource::Touch => now.0 - self.started.0,
                HoldSource::Key(_) => duration,
            };
            if !(0.0..=10.0).contains(&age) {
                hold.expired = true;
            }
            if hold.expired || duration < 0.7 || !self.clicks.assigned(&hold.token, Gesture::Long) {
                continue;
            }
            let token = hold.token.with_gesture(Gesture::Long);
            let repeat = self
                .clicks
                .mapping
                .0
                .get(&token)
                .is_some_and(|action| action.repeats());
            if let Some(last) = hold.last {
                if !repeat || now.0 - last.0 < 0.5 {
                    continue;
                }
                self.repeated.insert(token.clone());
            }
            hold.last = Some(now);
            result.push(token);
        }
        result
    }

    pub(crate) fn key_token(&self, key: u16) -> Token {
        match (self.profile, key) {
            (Profile::YiserJ6, 115) => Token("1".to_owned()),
            (Profile::Generic | Profile::YiserJ6, _) => Token(format!("key:{key}")),
        }
    }
}
