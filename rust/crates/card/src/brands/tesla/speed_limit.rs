use super::{state::State, Error};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct SpeedLimit {
    pub configured: bool,
    pub last_tx_nanos: u64,
    pub pending_since_nanos: u64,
    pub pending_direction: i32,
    pub pending_speed_display: i64,
    pub planned_target_display: i64,
    pub feedback_blocked_signature: Option<(i64, i64)>,
    pub manual_adjustment_counter_seen: Option<u64>,
    pub resume_gesture_counter_seen: Option<u64>,
    pub manual_override_active: bool,
    pub last_current_display: Option<i64>,
    pub target_change_nanos: u64,
    pub target_stabilizing: bool,
}
pub fn create_wheel_frame(template: &[u8], tick: i32) -> Result<[u8; 8], Error> {
    let mut data = <[u8; 8]>::try_from(template).map_err(|_| Error::WheelTemplate)?;
    if data[0] & 3 != 1 || data[3] & 0x3f != 0 {
        return Err(Error::WheelTemplate);
    }
    let raw = match tick {
        -1 => 0x3f,
        1 => 1,
        _ => return Err(Error::WheelTick),
    };
    data[3] = (data[3] & 0xc0) | raw;
    Ok(data)
}
fn age(now: u64, then: u64) -> i128 {
    i128::from(now) - i128::from(then)
}
impl SpeedLimit {
    fn reset_pending(&mut self) {
        self.pending_since_nanos = 0;
        self.pending_direction = 0;
    }
    fn reset(&mut self, clear: bool) {
        self.reset_pending();
        self.feedback_blocked_signature = None;
        self.last_current_display = None;
        self.planned_target_display = 0;
        self.target_change_nanos = 0;
        self.target_stabilizing = false;
        if clear {
            self.manual_override_active = false;
        }
    }
    fn display(speed: f64, mph: bool) -> Result<i64, Error> {
        (speed.max(0.) / if mph { 0.44704 } else { 1. / 3.6 } + 0.5)
            .to_i64()
            .ok_or(Error::Numeric)
    }
    pub fn update(
        &mut self,
        cc: car_control::Reader<'_>,
        state: &State,
        now: u64,
    ) -> Result<Vec<Frame>, Error> {
        let e = &state.extras;
        let (manual_changed, resume_changed) = if self.manual_adjustment_counter_seen.is_none() {
            (false, false)
        } else {
            (
                self.manual_adjustment_counter_seen
                    != Some(e.tesla_manual_speed_adjustment_counter),
                self.resume_gesture_counter_seen != Some(e.tesla_speed_auto_resume_gesture_counter),
            )
        };
        self.manual_adjustment_counter_seen = Some(e.tesla_manual_speed_adjustment_counter);
        self.resume_gesture_counter_seen = Some(e.tesla_speed_auto_resume_gesture_counter);
        let cs = state.out.get_root_as_reader::<car_state::Reader>()?;
        let cruise = cs.get_cruise_state()?;
        let fresh = age(now, e.tesla_speed_limit_target_nanos) <= 500_000_000;
        if !self.configured
            || !cc.get_enabled()
            || cc.get_cruise_control()?.get_cancel()
            || !cruise.get_enabled()
        {
            self.reset(true);
            return Ok(Vec::new());
        }
        if resume_changed {
            self.manual_override_active = false;
        } else if manual_changed {
            self.manual_override_active = true;
            self.reset_pending();
        }
        if cs.get_brake_pressed() || !e.tesla_speed_limit_target_valid || !fresh {
            self.reset(false);
            return Ok(Vec::new());
        }
        let mph = e.tesla_speed_units == "MPH";
        let current = Self::display(f64::from(cruise.get_speed_cluster()), mph)?;
        let target = Self::display(e.tesla_speed_limit_target, mph)?;
        let signature = (target, current);
        if target != self.planned_target_display {
            self.reset_pending();
            self.feedback_blocked_signature = None;
            self.planned_target_display = target;
            self.target_change_nanos = now;
            self.target_stabilizing = true;
        }
        if self.manual_override_active {
            return Ok(Vec::new());
        }
        if self.target_stabilizing {
            if age(now, self.target_change_nanos) < 500_000_000 {
                return Ok(Vec::new());
            }
            self.target_stabilizing = false;
        }
        if self.pending_direction != 0 {
            let received = current != self.pending_speed_display;
            let timed = age(now, self.pending_since_nanos) >= 1_200_000_000;
            if !received && !timed {
                return Ok(Vec::new());
            }
            self.reset_pending();
            if !received {
                self.feedback_blocked_signature = Some(signature);
                return Ok(Vec::new());
            }
        }
        if let Some(blocked) = self.feedback_blocked_signature {
            if blocked == signature {
                return Ok(Vec::new());
            }
            self.feedback_blocked_signature = None;
        }
        let remaining = target - current;
        if remaining == 0 || (self.last_tx_nanos != 0 && age(now, self.last_tx_nanos) < 500_000_000)
        {
            return Ok(Vec::new());
        }
        let Some(template) = e.tesla_speed_button_template else {
            return Ok(Vec::new());
        };
        if age(now, e.tesla_speed_button_template_nanos) > 1_500_000_000 {
            return Ok(Vec::new());
        }
        let direction = if remaining > 0 { 1 } else { -1 };
        let data = create_wheel_frame(&template, direction)?;
        self.last_tx_nanos = now;
        self.pending_since_nanos = now;
        self.pending_direction = direction;
        self.pending_speed_display = current;
        Ok(vec![Frame {
            address: 0x3c2,
            data: data.to_vec(),
            bus: 1,
        }])
    }
}
