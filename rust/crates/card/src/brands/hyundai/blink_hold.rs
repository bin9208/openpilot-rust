//! Final model lane-crossing turn-signal hold.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct BlinkHold {
    pub crossed: bool,
    pub released: bool,
    pub y_min: Option<f64>,
}
impl BlinkHold {
    pub fn update(&mut self, desire: u16, lines: Option<[f64; 2]>, requested: u8) -> u8 {
        let hold = if ![3, 4].contains(&desire) {
            *self = Self::default();
            0
        } else if self.released {
            0
        } else {
            if let Some(lines) = lines {
                let (target, other) = if desire == 3 {
                    (lines[0], lines[1])
                } else {
                    (lines[1], lines[0])
                };
                let minimum = self.y_min.map_or(target.abs(), |previous| {
                    if target.abs() < previous {
                        target.abs()
                    } else {
                        previous
                    }
                });
                self.y_min = Some(minimum);
                if !self.crossed && minimum < 1. && target.abs() - minimum > 1.5 {
                    self.crossed = true;
                }
                if self.crossed && other.abs() >= (0.6 - 0.5) * (target.abs() + other.abs()) {
                    self.released = true;
                }
            }
            if self.released {
                0
            } else if desire == 3 {
                3
            } else {
                4
            }
        };
        match requested {
            1 => 3,
            2 => 4,
            _ => hold,
        }
    }
}
