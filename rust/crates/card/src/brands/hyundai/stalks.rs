//! Turn-signal stalk edges and one-touch detent, independent of lamp timing.
#[derive(Default, serde::Deserialize, serde::Serialize)]
pub struct Stalks {
    pub left_stalk_prev: bool,
    pub right_stalk_prev: bool,
    pub left_blinker_stalk_count: u8,
    pub right_blinker_stalk_count: u8,
}
impl Stalks {
    pub fn update(&mut self, sample: Option<[bool; 4]>) -> [u8; 3] {
        let mut lever = 0;
        if let Some([left, right, left_tap, right_tap]) = sample {
            if left && !self.left_stalk_prev {
                self.left_blinker_stalk_count = self.left_blinker_stalk_count.wrapping_add(1);
            }
            if right && !self.right_stalk_prev {
                self.right_blinker_stalk_count = self.right_blinker_stalk_count.wrapping_add(1);
            }
            self.left_stalk_prev = left;
            self.right_stalk_prev = right;
            lever = if left_tap || right_tap {
                1
            } else if left || right {
                2
            } else {
                0
            };
        }
        [
            lever,
            self.left_blinker_stalk_count,
            self.right_blinker_stalk_count,
        ]
    }
}
