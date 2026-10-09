use super::state::State;
use num_traits::ToPrimitive;
use openpilot_cereal::{
    car_capnp::car_state::{self, GearShifter},
    log_capnp::{event, onroad_event::EventName},
};

fn delta(first: u64, second: u64) -> Result<f64, capnp::Error> {
    (i128::from(first) - i128::from(second))
        .to_f64()
        .map(|value| value / 1e9)
        .ok_or_else(|| capnp::Error::failed("timestamp conversion failed".into()))
}
struct Segment {
    anchor_mono: u64,
    anchor_wall: f64,
    previous_mono: Option<u64>,
    last_car: Option<[bool; 3]>,
    enabled: bool,
    active: bool,
}
impl State {
    pub fn segment(&mut self, data: &[u8]) -> Result<(), capnp::Error> {
        let mut input = data;
        let mut local = Segment {
            anchor_mono: 0,
            anchor_wall: 0.0,
            previous_mono: None,
            last_car: None,
            enabled: self.enabled,
            active: false,
        };
        while !input.is_empty() {
            let message =
                capnp::serialize::read_message(&mut input, capnp::message::ReaderOptions::new())?;
            let value = message.get_root::<event::Reader<'_>>()?;
            self.event(value, &mut local)?;
        }
        Ok(())
    }
    fn event(&mut self, value: event::Reader<'_>, local: &mut Segment) -> Result<(), capnp::Error> {
        let which = value.which().map_err(capnp::Error::from)?;
        let mono = value.get_log_mono_time();
        if let event::InitData(data) = &which {
            if let Ok(data) = data {
                let wall = data.get_wall_time_nanos();
                if wall != 0 {
                    local.anchor_mono = mono;
                    local.anchor_wall = wall.to_f64().unwrap_or(0.0) / 1e9;
                }
            }
            return Ok(());
        }
        if local.anchor_wall == 0.0 {
            if let event::Clocks(Ok(data)) = &which {
                let wall = data.get_wall_time_nanos();
                if wall != 0 {
                    local.anchor_mono = mono;
                    local.anchor_wall = wall.to_f64().unwrap_or(0.0) / 1e9;
                }
            }
        }
        let wall = if local.anchor_wall != 0.0 {
            local.anchor_wall + delta(mono, local.anchor_mono)?
        } else {
            0.0
        };
        match which {
            event::SelfdriveState(data) => {
                let data = data?;
                local.enabled = data.get_enabled();
                local.active = data.get_active();
                if self.enabled && !local.enabled && wall != 0.0 {
                    self.disengage_count += 1;
                    let reason = match local.last_car {
                        None => "other",
                        Some(_) if wall - self.last_cancel <= 1.0 => "button",
                        Some([true, _, _]) => "brake",
                        Some([_, true, _]) => "steer",
                        Some([_, _, true]) => "gas",
                        Some(_) => "other",
                    };
                    if let Some((_, count)) =
                        self.causes.iter_mut().find(|(name, _)| *name == reason)
                    {
                        *count += 1;
                    } else {
                        self.causes.push((reason, 1));
                    }
                    if self.disengages.len() < 60 {
                        self.disengages.push((wall, reason));
                    }
                }
                self.enabled = local.enabled;
            }
            event::OnroadEvents(data) => {
                let mut present = [false; 3];
                if let Ok(data) = data {
                    for item in data {
                        let Ok(name) = item.get_name() else { break };
                        match name {
                            EventName::Fcw | EventName::StockFcw => present[0] = true,
                            EventName::Ldw => present[1] = true,
                            EventName::TooDistracted => present[2] = true,
                            _ => {}
                        }
                    }
                }
                for (index, on) in present.into_iter().enumerate() {
                    if on && !self.warn_previous[index] {
                        self.warn_counts[index] += 1;
                    }
                    self.warn_previous[index] = on;
                }
            }
            event::CarParams(Ok(data)) => {
                let ratio = f64::from(data.get_steer_ratio());
                let wheelbase = f64::from(data.get_wheelbase());
                if ratio > 0.0 {
                    self.steer_ratio = ratio;
                }
                if wheelbase > 0.0 {
                    self.wheelbase = wheelbase;
                }
            }
            event::CarState(data) => self.car(data?, mono, wall, local)?,
            _ => {}
        }
        Ok(())
    }
    fn car(
        &mut self,
        car: car_state::Reader<'_>,
        mono: u64,
        wall: f64,
        local: &mut Segment,
    ) -> Result<(), capnp::Error> {
        local.last_car = Some([
            car.get_brake_pressed(),
            car.get_steering_pressed(),
            car.get_gas_pressed(),
        ]);
        if let Ok(buttons) = car.get_button_events() {
            for button in buttons {
                let Ok(kind) = button.get_type() else { break };
                if kind == car_state::button_event::Type::Cancel && button.get_pressed() {
                    self.last_cancel = wall;
                }
            }
        }
        if wall == 0.0 {
            return Ok(());
        }
        if self.first_wall == 0.0 {
            self.first_wall = wall;
        }
        self.last_wall = wall;
        let dt = match local.previous_mono {
            Some(previous) => delta(mono, previous)?.clamp(0.0, 0.5),
            None => 0.0,
        };
        local.previous_mono = Some(mono);
        let speed = f64::from(car.get_v_ego());
        let acceleration = f64::from(car.get_a_ego());
        let gear = car.get_gear_shifter().map_err(capnp::Error::from)?;
        let driving = matches!(
            gear,
            GearShifter::Drive
                | GearShifter::Low
                | GearShifter::Reverse
                | GearShifter::Sport
                | GearShifter::Eco
                | GearShifter::Manumatic
        ) || speed > 0.3;
        if speed > self.max_speed {
            self.max_speed = speed;
        }
        self.accel.feed(wall, acceleration);
        self.decel.feed(wall, acceleration);
        let mut corner = false;
        if speed >= 5.5 {
            let yaw = f64::from(car.get_yaw_rate());
            let lat = if yaw.abs() > 1e-3 {
                (speed * yaw).abs()
            } else {
                let radians =
                    (f64::from(car.get_steering_angle_deg()) / self.steer_ratio).to_radians();
                if radians.is_infinite() {
                    return Err(capnp::Error::failed("math domain error".into()));
                }
                (speed * speed * radians.tan() / self.wheelbase).abs()
            };
            if lat > self.max_lat {
                self.max_lat = lat;
            }
            corner = lat >= 3.0;
        }
        if corner && !self.cornering {
            self.corner_count += 1;
        }
        self.cornering = corner;
        let standstill = car.get_standstill();
        if standstill && !self.standstill {
            self.stop_count += 1;
        }
        self.standstill = standstill;
        if standstill {
            self.stop += dt;
        }
        self.used_log = true;
        if local.enabled {
            self.auto_enabled += dt;
            self.auto_distance += speed * dt;
            if local.active {
                self.auto_active += dt;
            }
            let pressed = car.get_steering_pressed();
            if pressed && !self.steering {
                self.steer_count += 1;
            }
            if pressed {
                self.steer += dt;
            }
            self.steering = pressed;
        } else {
            self.steering = false;
            if driving {
                self.manual += dt;
                if car.get_gas_pressed() {
                    self.manual_gas += dt;
                }
                if car.get_brake_pressed() {
                    self.manual_brake += dt;
                }
            }
        }
        if driving {
            self.distance += speed * dt;
        }
        Ok(())
    }
}
