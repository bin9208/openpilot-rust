use crate::geometry::{MouseEvent, Rect};
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub enum State {
    #[default]
    Idle,
    Dragging,
}
#[derive(Debug, serde::Serialize)]
pub struct Scroll {
    pub state: State,
    pub offset: f64,
    pub velocity: f64,
    last_y: f64,
    start_y: f64,
    last_time: f64,
    velocity_initialized: bool,
    fps: f64,
}
impl Default for Scroll {
    fn default() -> Self {
        Self {
            state: State::Idle,
            offset: 0.0,
            velocity: 0.0,
            last_y: 0.0,
            start_y: 0.0,
            last_time: 0.0,
            velocity_initialized: true,
            fps: 20.0,
        }
    }
}
impl Scroll {
    pub fn with_fps(fps: f64) -> Result<Self, crate::Error> {
        if !fps.is_finite() || fps <= 0.0 {
            return Err(crate::Error::Contract("scroll FPS must be positive"));
        }
        Ok(Self {
            fps,
            ..Self::default()
        })
    }
    pub fn set_offset(&mut self, position: f64) {
        self.offset = position;
        self.velocity = 0.0;
        self.state = State::Idle;
    }
    pub fn touch_valid(&self) -> bool {
        matches!(self.state, State::Idle) && self.velocity.abs() < 120.0
    }

    fn velocity_update(&mut self, value: f64) {
        self.velocity = if self.velocity_initialized {
            let alpha = (1.0 / self.fps) / (0.05 + 1.0 / self.fps);
            (1.0 - alpha) * self.velocity + alpha * value
        } else {
            self.velocity_initialized = true;
            value
        };
    }
    pub fn update(
        &mut self,
        bounds: Rect,
        content_height: f32,
        events: &[MouseEvent],
        wheel: f32,
    ) -> f32 {
        let maximum = (f64::from(content_height) - f64::from(bounds.height)).max(0.0);
        for event in events.iter().filter(|event| event.slot == 0) {
            match self.state {
                State::Idle => {
                    if bounds.contains(event.pos) {
                        if event.pressed {
                            self.start_y = f64::from(event.pos.y);
                            if self.velocity.abs() > 120.0 {
                                self.state = State::Dragging;
                                self.velocity_initialized = false;
                            }
                        }
                        if event.down && (f64::from(event.pos.y) - self.start_y).abs() > 12.0 {
                            self.state = State::Dragging;
                            self.velocity_initialized = false;
                        }
                    }
                }
                State::Dragging => {
                    if event.released {
                        self.state = State::Idle;
                    } else {
                        let mut delta = f64::from(event.pos.y) - self.last_y;
                        if self.offset > 0.0 || self.offset < -maximum {
                            delta /= 3.0;
                        }
                        self.offset += delta;
                        let dt = event.time - self.last_time;
                        if dt > 0.0 {
                            self.velocity_update(delta / dt);
                        }
                    }
                }
            }
            self.last_time = event.time;
            self.last_y = f64::from(event.pos.y);
        }
        self.offset += f64::from(wheel) * 50.0;
        match self.state {
            State::Idle => {
                let outside = self.offset > 0.0 || self.offset < -maximum;
                self.velocity = if self.velocity.abs() > 2.0 {
                    self.velocity
                        * (-5.0_f64 / self.fps)
                            .exp()
                            .powi(if outside { 2 } else { 1 })
                } else {
                    0.0
                };
                if outside {
                    self.offset = (1.0 - (1.0 / self.fps) / (0.1 + 1.0 / self.fps)) * self.offset
                        + ((1.0 / self.fps) / (0.1 + 1.0 / self.fps))
                            * if self.offset > 0.0 { 0.0 } else { -maximum };
                }
                self.offset += self.velocity / self.fps;
            }
            State::Dragging => {
                if events.is_empty() {
                    self.velocity_update(0.0);
                }
            }
        }
        if self.offset.abs() < 0.01 {
            self.offset = 0.0;
        } else if (self.offset + maximum).abs() < 0.01 {
            self.offset = -maximum;
        }
        crate::number::float(self.offset)
    }
}
