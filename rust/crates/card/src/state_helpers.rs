use openpilot_cereal::car_capnp::car_state::{self, GearShifter};

pub struct SpeedFilter {
    state: [f64; 2],
    gain: [f64; 2],
}
impl SpeedFilter {
    pub fn state(&self) -> [f64; 2] {
        self.state
    }
    pub fn new() -> Result<Self, crate::vehicle_params::Error> {
        Ok(Self {
            state: [0.; 2],
            gain: crate::vehicle_params::speed_gain()?,
        })
    }

    pub fn update(&mut self, raw: f64) -> [f64; 2] {
        if (raw - self.state[0]).abs() > 2. {
            self.state = [raw, 0.];
        }
        let [speed, acceleration] = self.state;
        let [k0, k1] = self.gain;
        self.state = [
            (1. - k0) * speed + 0.01 * acceleration + k0 * raw,
            -k1 * speed + acceleration + k1 * raw,
        ];
        self.state
    }
}

#[derive(Default)]
pub struct Blinkers {
    counts: [u32; 2],
    previous: [bool; 2],
}
impl Blinkers {
    pub fn lamp(&mut self, time: u32, left: bool, right: bool) -> [bool; 2] {
        for (count, lamp) in self.counts.iter_mut().zip([left, right]) {
            *count = if lamp { time } else { count.saturating_sub(1) };
        }
        self.counts.map(|count| count > 0)
    }

    pub fn stalk(&mut self, time: u32, left: bool, right: bool) -> [bool; 2] {
        if left {
            self.counts[1] = 0;
            if !self.previous[0] {
                self.counts[0] = time;
            }
        }
        if right {
            self.counts[0] = 0;
            if !self.previous[1] {
                self.counts[1] = time;
            }
        }
        self.counts = self.counts.map(|count| count.saturating_sub(1));
        self.previous = [left, right];
        [left || self.counts[0] > 0, right || self.counts[1] > 0]
    }
}

#[derive(Default)]
pub struct SteeringPressed {
    count: u32,
}
impl SteeringPressed {
    pub fn update(&mut self, pressed: bool, minimum: u32) -> bool {
        self.count = if pressed {
            self.count.saturating_add(1).min(minimum.saturating_add(1))
        } else {
            0
        };
        self.count > minimum
    }
}

pub fn parse_gear(gear: Option<&str>) -> GearShifter {
    match gear.map(str::to_uppercase).as_deref() {
        Some("P" | "PARK") => GearShifter::Park,
        Some("R" | "REVERSE") => GearShifter::Reverse,
        Some("N" | "NEUTRAL") => GearShifter::Neutral,
        Some("E" | "ECO") => GearShifter::Eco,
        Some("T" | "MANUAL") => GearShifter::Manumatic,
        Some("D" | "DRIVE") => GearShifter::Drive,
        Some("S" | "SPORT") => GearShifter::Sport,
        Some("L" | "LOW") => GearShifter::Low,
        Some("B" | "BRAKE") => GearShifter::Brake,
        _ => GearShifter::Unknown,
    }
}

pub fn button_enable(
    pcm_cruise: bool,
    events: capnp::struct_list::Reader<'_, car_state::button_event::Owned>,
) -> Result<bool, capnp::NotInSchema> {
    if !pcm_cruise {
        for event in events {
            if matches!(
                event.get_type()?,
                car_state::button_event::Type::AccelCruise
                    | car_state::button_event::Type::DecelCruise
            ) && !event.get_pressed()
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn wheel_speeds(values: [f64; 4], wheel_speed_factor: f64, unit: f64) -> [f64; 4] {
    let factor = unit * wheel_speed_factor;
    values.map(|value| value * factor)
}
