use super::{can_hud::Meb, controller::Controller, state::State, Error};
use openpilot_cereal::car_capnp::{car_control, car_state};
use openpilot_control_policy::math::maximum;
impl Controller {
    pub(super) fn meb_hud(
        &mut self,
        state: &State,
        cc: car_control::Reader<'_>,
    ) -> Result<Meb, Error> {
        let out = state.out.get_root_as_reader::<car_state::Reader>()?;
        let hud = cc.get_hud_control()?;
        let override_ = cc.get_cruise_control()?.get_override() || out.get_gas_pressed();
        let status = if out.get_acc_faulted() {
            6
        } else if cc.get_enabled() {
            if override_ {
                4
            } else {
                3
            }
        } else if out.get_cruise_state()?.get_available() {
            2
        } else {
            0
        };
        let lead = f64::from(hud.get_lead_distance());
        let distance = if lead != 0. { maximum(8., lead) } else { 0. };
        let gap = maximum(8., f64::from(out.get_v_ego()) * 1.45);
        let mut event = if state.extras.esp_hold_confirmation && status == 3 {
            3
        } else {
            0
        };
        let mut speed_limit = 0.;
        let mut event_speed = 0.;
        let navi_speed = f64::from(hud.get_navi_event_speed());
        if matches!(status, 3 | 4) {
            if hud.get_navi_speed_limit() > 0 {
                speed_limit = f64::from(hud.get_navi_speed_limit()) * (1. / 3.6);
                event = 4;
            } else if hud.get_navi_event_type() == 1 && navi_speed != 0. {
                event = if navi_speed > 0. { 7 } else { 8 };
                event_speed = navi_speed.abs();
            } else if hud.get_navi_event_type() == 2 && navi_speed > 0. {
                event = 9;
                event_speed = navi_speed;
            } else if hud.get_navi_event_type() == 3 && navi_speed > 0. {
                event = 11;
                event_speed = navi_speed;
            } else if hud.get_navi_event_type() == 4 {
                event = 10;
                event_speed = navi_speed;
            } else if hud.get_navi_event_type() == 5 {
                event = 13;
            }
        }
        let mut icon = 0;
        if event >= 6 {
            if event != self.history.navi_event_last {
                self.history.navi_banner_frames = 20;
            }
            self.history.navi_event_last = event;
            if self.history.navi_banner_frames > 0 {
                self.history.navi_banner_frames -= 1;
            } else {
                icon = match event {
                    7 => 7,
                    8 => 6,
                    value => value - 1,
                };
                event = 0;
                event_speed = 0.;
            }
        } else {
            self.history.navi_event_last = 0;
            self.history.navi_banner_frames = 0;
            if hud.get_lead_limiting() != self.history.lead_limit_disp {
                self.history.lead_limit_cnt = self
                    .history
                    .lead_limit_cnt
                    .checked_add(1)
                    .ok_or(Error::Numeric)?;
                if self.history.lead_limit_cnt >= 32 {
                    self.history.lead_limit_disp = hud.get_lead_limiting();
                    self.history.lead_limit_cnt = 0;
                }
            } else {
                self.history.lead_limit_cnt = 0;
            }
            if event == 0 && matches!(status, 3 | 4) && hud.get_navi_event_type() == 8 {
                let road = navi_speed.abs();
                if road != self.history.road_limit_last {
                    self.history.road_banner_frames = 20;
                }
                self.history.road_limit_last = road;
                if self.history.road_banner_frames > 0 && road > 0. {
                    self.history.road_banner_frames -= 1;
                    event = 5;
                    speed_limit = road * (1. / 3.6);
                } else if !self.history.lead_limit_disp {
                    icon = 4;
                }
            } else {
                self.history.road_banner_frames = 0;
            }
        }
        Ok(Meb {
            status,
            speed: f64::from(hud.get_set_speed()) * 3.6,
            lead: hud.get_lead_visible(),
            bars: f64::from(hud.get_lead_distance_bars()),
            distance,
            gap,
            event,
            speed_limit,
            event_speed,
            icon,
        })
    }
}
