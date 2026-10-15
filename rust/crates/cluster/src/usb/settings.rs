use super::{write::NoAck, Display, Runtime};
use std::time::Duration;

impl<R: Runtime> Display<R> {
    pub(super) fn brightness_raw(&self) -> i64 {
        // Preserve int(brightness / 100 * 102), including source floating truncation.
        num_traits::ToPrimitive::to_i64(&(f64::from(self.options.brightness) / 100.0 * 102.0))
            .unwrap_or(0)
    }

    pub fn set_brightness(&mut self, value: i32, force: bool) -> bool {
        let value = value.clamp(0, 100);
        if value == self.options.brightness && !force {
            return false;
        }
        self.options.brightness = value;
        if !self.is_open() {
            return false;
        }
        self.setting(14, self.brightness_raw());
        true
    }

    pub fn set_orientation(&mut self, value: i32, force: bool) -> bool {
        let value = match value {
            0 => 0,
            2 => 2,
            _ => return false,
        };
        if value == self.orientation && !force {
            return false;
        }
        self.orientation = value;
        if !self.is_open() {
            return false;
        }
        self.setting(13, i64::from(value));
        true
    }

    pub fn set_display_fps(&mut self, value: i32, force: bool) -> bool {
        let value = value.clamp(0, 255);
        if value == self.options.display_fps && !force {
            return false;
        }
        self.options.display_fps = value;
        if !self.is_open() || value == 0 {
            return false;
        }
        self.optional_command(15, &[(8, i64::from(value))], NoAck::FRAME_RATE);
        true
    }

    fn setting(&mut self, id: u8, value: i64) {
        self.optional_command(10, &[], NoAck::NONE);
        self.runtime.sleep(Duration::from_millis(20));
        self.optional_command(id, &[(8, value)], NoAck::NONE);
    }
}
