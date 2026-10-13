use crate::scene::math::clamp;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Playback {
    pub base: f64,
    pub wall: f64,
    pub speed: f64,
    pub paused: bool,
    pub waiting: bool,
    pub next_retry: f64,
    sound_active: bool,
}
impl Playback {
    #[must_use]
    pub fn new(start: f64, duration: f64, now: f64, speed: f64) -> Self {
        Self {
            base: clamp(start, 0.0, duration),
            wall: now,
            speed,
            paused: false,
            waiting: false,
            next_retry: 0.0,
            sound_active: false,
        }
    }
    #[must_use]
    pub fn time(&self, now: f64) -> f64 {
        if self.paused {
            self.base
        } else {
            self.base + (now - self.wall) * self.speed
        }
    }
    pub fn toggle(&mut self, now: f64, time: f64) {
        self.paused = !self.paused;
        self.base = time;
        self.wall = now;
    }
    pub fn seek(&mut self, now: f64, time: f64) {
        self.pause(now, time);
        self.waiting = false;
        self.reset_cutin();
    }
    pub fn pause(&mut self, now: f64, time: f64) {
        self.base = time;
        self.wall = now;
        self.paused = true;
    }
    pub fn finish(&mut self, now: f64, duration: f64) {
        self.pause(now, duration);
        self.waiting = true;
    }
    pub fn retry_due(&mut self, now: f64) -> bool {
        if now < self.next_retry {
            return false;
        }
        self.next_retry = now + 1.0;
        true
    }
    pub fn resume_loop(&mut self, now: f64) {
        if self.waiting {
            self.waiting = false;
            self.paused = false;
            self.base = 0.0;
            self.wall = now;
        }
    }
    pub fn reset_cutin(&mut self) {
        self.sound_active = false;
    }
    pub fn cutin(&mut self, sound: bool, enabled: bool, now: f64, time: f64) -> bool {
        let sound = enabled && sound;
        let triggered = sound && !self.sound_active;
        self.sound_active = sound;
        if triggered {
            self.pause(now, time);
        }
        triggered
    }
}
