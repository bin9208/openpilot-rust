use crate::{
    decoder::{Decoder, Point, Touch},
    holds,
    types::{Event, Gesture, HoldSource, Seconds, Token},
};

impl Decoder {
    pub fn feed(&mut self, event: Event) -> Vec<Token> {
        self.repeated.clear();
        self.frame_open = event.kind != 0 || event.code != 0;
        if event.kind == 0 && event.code == 3 {
            self.track = None;
            self.down.clear();
            self.pending_keys.clear();
            self.clicks.pending.clear();
            self.holds.clear();
            self.dropped = true;
            return Vec::new();
        }
        match (event.kind, event.code) {
            (3, 0) => self.x = event.value,
            (3, 1) => self.y = event.value,
            (1, 330) => self.touch = event.value != 0,
            (1, code) if !(256..352).contains(&code) => match event.value {
                1 => {
                    self.down.entry(code).or_insert(event.at);
                }
                0 => {
                    if let Some(started) = self.down.shift_remove(&code) {
                        self.pending_keys
                            .push((code, Seconds(event.at.0 - started.0)));
                    }
                }
                _ => {}
            },
            _ => {}
        }
        if self.frame_open {
            return Vec::new();
        }
        if self.dropped {
            self.pending_keys.clear();
            if !self.touch && self.down.is_empty() {
                self.dropped = false;
            }
            return Vec::new();
        }
        let mut releases = Vec::new();
        let pending_keys = std::mem::take(&mut self.pending_keys);
        for (key, duration) in pending_keys {
            let hold = self.holds.shift_remove(&HoldSource::Key(key));
            if hold.is_none_or(|hold| hold.last.is_none() && !hold.expired && !hold.short_fired) {
                releases.push((self.key_token(key), duration));
            }
        }
        let mut tokens = Vec::new();
        let down: Vec<(u16, Seconds)> = self.down.iter().map(|(k, s)| (*k, *s)).collect();
        for (key, started) in down {
            let token = self.key_token(key);
            holds::update(&mut self.holds, HoldSource::Key(key), Some(token), started);
            if let Some(hold) = self.holds.get(&HoldSource::Key(key)) {
                let fire = !hold.short_fired && !hold.expired && self.immediate(&hold.token);
                let token = hold.token.clone();
                if fire {
                    tokens.push(token);
                    if let Some(hold) = self.holds.get_mut(&HoldSource::Key(key)) {
                        hold.short_fired = true;
                    }
                }
            }
        }
        if self.touch {
            let track = self.track.get_or_insert_with(|| {
                self.started = event.at;
                Touch {
                    start: Point {
                        x: self.x,
                        y: self.y,
                    },
                    last: Point {
                        x: self.x,
                        y: self.y,
                    },
                }
            });
            track.last = Point {
                x: self.x,
                y: self.y,
            };
            let token = self.touch_token();
            holds::update(&mut self.holds, HoldSource::Touch, token, event.at);
            // A touch may begin over a tap target before becoming a swipe:
            // only a confirmed direction fires early; taps fire on release.
            let moving = self.track.as_ref().is_some_and(|t| {
                (i64::from(t.last.x) - i64::from(t.start.x))
                    .abs()
                    .max((i64::from(t.last.y) - i64::from(t.start.y)).abs())
                    >= 120
            });
            if moving {
                if let Some(hold) = self.holds.get(&HoldSource::Touch) {
                    if !hold.short_fired && !hold.expired && self.immediate(&hold.token) {
                        tokens.push(hold.token.clone());
                        if let Some(hold) = self.holds.get_mut(&HoldSource::Touch) {
                            hold.short_fired = true;
                        }
                    }
                }
            }
        } else if self.track.is_some() {
            let token = self.touch_token();
            let hold = self.holds.shift_remove(&HoldSource::Touch);
            self.track = None;
            let duration = event.at.0 - self.started.0;
            if let Some(token) = token {
                if hold.is_none_or(|hold| hold.last.is_none() && !hold.expired && !hold.short_fired)
                    && (0.0..=10.0).contains(&duration)
                    && (duration <= 1.5 || self.clicks.assigned(&token, Gesture::Long))
                {
                    releases.push((token, Seconds(duration)));
                }
            }
        }
        for (token, duration) in releases {
            tokens.extend(self.clicks.release(token, duration, event.at));
        }
        tokens
    }
}
