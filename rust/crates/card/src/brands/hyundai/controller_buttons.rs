use super::{
    canfd_buttons,
    controller_settings::Settings,
    flags as f, legacy_steering,
    state::State,
    wire::{CanWriter, Values},
    Error,
};
use num_traits::ToPrimitive;
use openpilot_can::Frame;
use openpilot_cereal::car_capnp::{car_control, car_state};
use std::collections::BTreeMap;

pub struct Buttons {
    pub diagnostics: Vec<String>,
    last_frame: i64,
    activation: u8,
    wait: i32,
    count: i32,
    previous_speed: f64,
    alternate_count: u32,
    pub main_trigger: i32,
    pub lfa_trigger: i32,
}

impl Default for Buttons {
    fn default() -> Self {
        Self {
            diagnostics: Vec::new(),
            last_frame: 0,
            activation: 0,
            wait: 12,
            count: 0,
            previous_speed: 0.,
            alternate_count: 0,
            main_trigger: 0,
            lfa_trigger: 0,
        }
    }
}

impl Buttons {
    pub fn toggle(&mut self, cc: car_control::Reader<'_>, state: &State) -> Result<(), Error> {
        self.main_trigger = (self.main_trigger - 1).max(-200);
        self.lfa_trigger = (self.lfa_trigger - 1).max(-200);
        let cs = state.out.get_root_as_reader::<car_state::Reader<'_>>()?;
        if cs.get_brake_hold_active() || cs.get_parking_brake() {
            self.main_trigger = -200;
            return Ok(());
        }
        if self.main_trigger == -200 && self.lfa_trigger == -200 {
            if cc.get_enabled() && !state.main_mode && cs.get_v_ego() > 3. {
                self.main_trigger = 6;
            } else if cc.get_lat_active() && state.lfa_icon == 0. {
                self.lfa_trigger = 6;
            }
        }
        Ok(())
    }

    pub fn spam(
        &mut self,
        input: (car_control::Reader<'_>, &State),
        settings: &Settings,
        frame: u32,
    ) -> Result<u8, Error> {
        let (cc, state) = input;
        let cs = state.out.get_root_as_reader::<car_state::Reader<'_>>()?;
        if cs.get_brake_pressed() || cs.get_brake_hold_active() || cs.get_parking_brake() {
            self.activation = 0;
            return Ok(0);
        }
        let hud = cc.get_hud_control()?;
        let cruise = cs.get_cruise_state()?;
        let resume = cc.get_cruise_control()?.get_resume();
        let unit = if state.metric { 3.6 } else { 1. / 0.44704 };
        let target = (f64::from(hud.get_set_speed()) * unit + 0.5).trunc();
        let current = (f64::from(cruise.get_speed()) * unit + 0.5).trunc();
        let activation_request = if cc.get_enabled() {
            !cruise.get_enabled()
        } else {
            cs.get_activate_cruise() > 0
        };
        let mut button = 0;
        let mut activated = false;
        if activation_request {
            if (hud.get_lead_visible() || f64::from(cs.get_v_ego()) * 3.6 > 10.)
                && self.activation == 0
            {
                button = 1;
                self.activation = 1;
                activated = true;
            }
        } else if cc.get_enabled() {
            if resume {
                button = 1;
            } else if target < current && current >= 31. && settings.speed_from_pcm != 1 {
                button = 2;
            } else if target > current && current < 160. && settings.speed_from_pcm != 1 {
                button = 1;
            }
        }
        if cs.get_brake_pressed() || cs.get_gas_pressed() {
            self.activation = 0;
        }
        if button == 0 {
            self.count = 0;
            self.previous_speed = current;
            return Ok(0);
        }
        let difference = self.previous_speed - current;
        if state.cruise_buttons.back().copied().unwrap_or(0) != 0 {
            self.last_frame = i64::from(frame);
            self.wait = settings.spam[1];
            self.count = 0;
        } else if self.count.abs() >= settings.spam[0] || difference.abs() > 0. {
            self.last_frame = i64::from(frame);
            self.wait = if self.count.abs() >= settings.spam[0] {
                settings.spam[1]
            } else {
                7
            };
            self.count = 0;
        }
        self.previous_speed = current;
        if i64::from(frame) - self.last_frame > i64::from(self.wait)
            || activated
            || (resume && frame.is_multiple_of(2))
        {
            self.count += if button == 1 { 1 } else { -1 };
            return Ok(button);
        }
        self.count = 0;
        Ok(0)
    }

    pub fn messages(
        &mut self,
        writer: &mut CanWriter,
        input: (car_control::Reader<'_>, &State),
        context: (&Settings, u32, &BTreeMap<&str, Values>),
    ) -> Result<Vec<Frame>, Error> {
        let (cc, state) = input;
        let (settings, frame, captures) = context;
        let cs = state.out.get_root_as_reader::<car_state::Reader<'_>>()?;
        if cs.get_brake_pressed() || cs.get_brake_hold_active() || cs.get_parking_brake() {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        if state.config.flags & f::CANFD == 0 {
            let Some(clu) = captures.get("clu11") else {
                return Ok(result);
            };
            if cc.get_cruise_control()?.get_cancel() {
                result.push(legacy_steering::clu_cancel(
                    writer,
                    clu,
                    (state.config.flags, frame),
                )?);
            }
            if self.last_frame != i64::from(frame) {
                let button = self.spam(input, settings, frame)?;
                if button > 0 {
                    result.push(legacy_steering::clu_button(
                        writer,
                        clu,
                        state.config.flags,
                        f64::from(button),
                    )?);
                }
            }
        } else {
            let alternate = state.config.flags & f::ALT_BUTTONS != 0;
            if alternate && captures.contains_key("cruise_buttons_msg") {
                self.alternate_count += 1;
            }
            if (f64::from(frame) - self.last_frame.to_f64().ok_or(Error::Numeric)?) * 0.01 > 0.25
                && cc.get_cruise_control()?.get_cancel()
            {
                self.diagnostics.push("cruiseControl.cancel222222".into());
                if !alternate {
                    for _ in 0..20 {
                        result.push(canfd_buttons::buttons(
                            writer,
                            state.config.bus,
                            state.buttons_counter + 1.,
                            4.,
                        )?);
                    }
                }
                self.last_frame = i64::from(frame);
            }
            if self.last_frame != i64::from(frame) && !alternate {
                let button = self.spam(input, settings, frame)?;
                if button > 0 {
                    let frame = canfd_buttons::buttons(
                        writer,
                        state.config.bus,
                        state.buttons_counter + 1.,
                        f64::from(button),
                    )?;
                    for _ in 0..settings.spam[2] {
                        result.push(frame.clone());
                    }
                    self.alternate_count += 1;
                }
            }
        }
        Ok(result)
    }
}
