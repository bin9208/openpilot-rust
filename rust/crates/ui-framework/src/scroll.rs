use crate::{
    geometry::{MouseEvent, Rect},
    widget::Property,
};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum ScrollState {
    #[default]
    Steady,
    Pressed,
    Manual,
    Auto,
}

pub struct ScrollPanel {
    pub horizontal: bool,
    pub handle_out_of_bounds: bool,
    pub enabled: Property<bool>,
    pub state: ScrollState,
    pub velocity: f64,
    offset: f32,
    initial: Option<MouseEvent>,
    previous: Option<MouseEvent>,
    samples: VecDeque<f64>,
    capacity: usize,
}
impl ScrollPanel {
    pub fn new(horizontal: bool, handle_out_of_bounds: bool, tici: bool) -> Self {
        Self {
            horizontal,
            handle_out_of_bounds,
            enabled: true.into(),
            state: ScrollState::Steady,
            velocity: 0.0,
            offset: 0.0,
            initial: None,
            previous: None,
            samples: VecDeque::new(),
            capacity: if tici { 12 } else { 6 },
        }
    }
    pub fn offset(&self) -> f64 {
        f64::from(self.offset)
    }
    pub fn set_offset(&mut self, value: f64) {
        use num_traits::ToPrimitive;
        self.offset = value.to_f32().unwrap_or(if value.is_sign_negative() {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        });
    }
    pub fn touch_valid(&self) -> bool {
        self.state != ScrollState::Manual
    }
    fn position(&self, event: MouseEvent) -> f64 {
        f64::from(if self.horizontal {
            event.pos.x
        } else {
            event.pos.y
        })
    }
    pub fn update(
        &mut self,
        bounds: Rect,
        content_size: f64,
        events: &[MouseEvent],
        dt: f64,
    ) -> f64 {
        let size = f64::from(if self.horizontal {
            bounds.width
        } else {
            bounds.height
        });
        for event in events {
            self.event(*event, bounds, size, content_size);
            self.previous = Some(*event);
        }
        let minimum = (size - content_size).min(0.0);
        let outside = self.offset() > 0.0 || self.offset() < minimum;
        match self.state {
            ScrollState::Steady if outside => self.state = ScrollState::Auto,
            ScrollState::Auto => {
                if outside && self.handle_out_of_bounds {
                    let target = if self.offset() > 0.0 { 0.0 } else { minimum };
                    let factor = 1.0 - (-10.0 * if dt == 0.0 { 1e-6 } else { dt }).exp();
                    let distance = target - self.offset();
                    self.set_offset(self.offset() + distance * factor);
                    self.velocity *= 1.0 - factor;
                    if distance.abs() < 1.0 && self.velocity.abs() < 10.0 {
                        self.set_offset(target);
                        self.velocity = 0.0;
                        self.state = ScrollState::Steady;
                    }
                } else if self.velocity.abs() < 10.0 {
                    self.velocity = 0.0;
                    self.state = ScrollState::Steady;
                }
                self.set_offset(self.offset() + self.velocity * dt);
                let tc = if self.handle_out_of_bounds {
                    0.18
                } else {
                    0.025
                };
                self.velocity *= 1.0 - dt / (tc + dt);
            }
            _ => {}
        }
        self.offset()
    }
    fn event(&mut self, event: MouseEvent, bounds: Rect, size: f64, content: f64) {
        let minimum = (size - content).min(0.0);
        let outside = self.offset() > 0.0 || self.offset() < minimum;
        let position = self.position(event);
        if !self.enabled.get() {
            self.state = ScrollState::Steady;
            self.velocity = 0.0;
            self.samples.clear();
            return;
        }
        match self.state {
            ScrollState::Steady => {
                if bounds.contains(event.pos) && event.pressed {
                    self.state = ScrollState::Pressed;
                    self.initial = Some(event);
                }
            }
            ScrollState::Pressed => {
                let Some(initial) = self.initial else { return };
                let difference = (position - self.position(initial)).abs();
                if event.released {
                    self.state = if outside {
                        ScrollState::Auto
                    } else if difference <= 12.0 {
                        ScrollState::Steady
                    } else {
                        ScrollState::Manual
                    };
                } else if difference > 12.0 {
                    self.state = ScrollState::Manual;
                }
            }
            ScrollState::Manual => {
                if event.released {
                    let high_deceleration = if self.samples.len() > 2 {
                        let values: Vec<_> = self
                            .samples
                            .iter()
                            .copied()
                            .enumerate()
                            .map(|(index, value)| (value.abs(), index))
                            .collect();
                        let compare = |a: &(f64, usize), b: &(f64, usize)| {
                            a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))
                        };
                        match (
                            values[..values.len() / 2]
                                .iter()
                                .max_by(|a, b| compare(a, b)),
                            values.iter().min_by(|a, b| compare(a, b)),
                        ) {
                            (Some(max), Some(min)) => min.0 * 3.0 < max.0 && max.1 < min.1,
                            _ => false,
                        }
                    } else {
                        false
                    };
                    self.velocity = weighted_velocity(&self.samples);
                    if outside || !(high_deceleration || self.velocity.abs() <= 180.0) {
                        self.state = ScrollState::Auto;
                    } else {
                        self.velocity = 0.0;
                        self.state = ScrollState::Steady;
                    }
                    self.samples.clear();
                } else if let Some(previous) = self.previous {
                    let mut delta = position - self.position(previous);
                    let elapsed = (event.time - previous.time).max(1e-6);
                    self.velocity = (delta / elapsed).clamp(-10000.0, 10000.0);
                    if self.samples.len() == self.capacity {
                        self.samples.pop_front();
                    }
                    self.samples.push_back(self.velocity);
                    if outside {
                        delta *= 0.25;
                    }
                    self.set_offset(self.offset() + delta);
                }
            }
            ScrollState::Auto => {
                if event.pressed {
                    if self.velocity.abs() <= 120.0 {
                        self.state = ScrollState::Pressed;
                        self.initial = Some(event);
                    } else {
                        self.state = ScrollState::Manual;
                        self.velocity = 0.0;
                    }
                }
            }
        }
    }
}
pub fn weighted_velocity(samples: &VecDeque<f64>) -> f64 {
    let count = samples.len();
    match count {
        0 => 0.0,
        1 => samples[0],
        2 => samples[0] * 0.7 + samples[1] * 0.3,
        _ => samples[count - 3] * 0.6 + samples[count - 2] * 0.35 + samples[count - 1] * 0.05,
    }
}
