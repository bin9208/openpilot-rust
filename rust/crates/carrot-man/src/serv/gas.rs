use super::{CarState, CarrotServ};

impl CarrotServ {
    pub fn gas_floor(
        &mut self,
        car: &CarState,
        desired: f64,
        source: &str,
        road_changed: bool,
        now: f64,
    ) -> (f64, String) {
        let rising = car.gas_pressed && !self.speed.event_gas_pressed;
        self.speed.event_gas_pressed = car.gas_pressed;
        let ego_kph = car.v_ego * 3.6;
        if matches!(source, "hda" | "hda_section" | "hda_bump" | "school") {
            let floor_active = source == "hda_bump" || self.settings.camera_control_mode == 2;
            if !floor_active {
                self.speed.gas_override = 0.;
            } else {
                if source != self.speed.last_source
                    || car.v_ego < 0.1
                    || desired > 150.
                    || car.brake_pressed
                    || road_changed
                {
                    self.speed.gas_override = 0.;
                }
                if self.speed.gas_override <= 0. {
                    if rising
                        && !car.brake_pressed
                        && car.v_ego >= 0.1
                        && desired <= 150.
                        && desired < ego_kph
                    {
                        self.speed.gas_override = ego_kph;
                    }
                } else if car.gas_pressed {
                    self.speed.gas_override = ego_kph.max(self.speed.gas_override);
                }
            }
            self.speed.last_source = source.into();
            let overriding = floor_active && desired < self.speed.gas_override;
            if source == "school" {
                if !overriding {
                    self.speed.school_override_since = None;
                } else if let Some(since) = self.speed.school_override_since {
                    if now - since >= 3. {
                        self.speed.school_suppressed = true;
                    }
                } else {
                    self.speed.school_override_since = Some(now);
                }
            } else if !self.speed.school_suppressed {
                self.speed.school_override_since = None;
            }
            return if overriding {
                (self.speed.gas_override, "gas".into())
            } else {
                (desired, source.into())
            };
        }
        if source != self.speed.last_source {
            self.speed.gas_override = 0.;
            self.speed.gas_pressed_state = car.gas_pressed;
        }
        if car.v_ego < 0.1
            || desired > 150.
            || matches!(source, "cam" | "section" | "police")
            || car.brake_pressed
            || road_changed
        {
            self.speed.gas_override = 0.;
        } else if source == "bump" {
            if self.speed.gas_override <= 0. {
                if rising && desired < ego_kph {
                    self.speed.gas_override = ego_kph;
                }
            } else if car.gas_pressed {
                self.speed.gas_override = ego_kph.max(self.speed.gas_override);
            }
        } else if car.gas_pressed && !self.speed.gas_pressed_state {
            self.speed.gas_override = ego_kph.max(self.speed.gas_override);
        } else {
            self.speed.gas_pressed_state = false;
        }
        self.speed.last_source = source.into();
        if desired < self.speed.gas_override {
            (self.speed.gas_override, "gas".into())
        } else {
            (desired, source.into())
        }
    }
}
