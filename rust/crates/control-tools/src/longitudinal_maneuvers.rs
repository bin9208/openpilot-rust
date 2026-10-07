//! Port of openpilot/tools/longitudinal_maneuvers/maneuversd.py; original MIT license applies.
use crate::{
    maneuver::{Action, Longitudinal, LongitudinalInput, Sequence, State},
    Error,
};
use openpilot_control_policy::math::maximum;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Input {
    pub speed: f64,
    pub active: bool,
    pub standstill: bool,
    pub cruise_standstill: bool,
    pub valid: bool,
}

pub struct Command {
    pub acceleration: f64,
    pub should_stop: bool,
    pub alert_text1: String,
    pub alert_text2: &'static str,
    pub selected: Option<usize>,
    pub state: Option<State>,
}

struct Preset {
    description: &'static str,
    owner: Longitudinal,
}

pub struct Controller {
    stopping_speed: f64,
    current: usize,
    presets: Vec<Preset>,
}

impl Controller {
    pub fn new(stopping_speed: f64) -> Result<Self, Error> {
        let step_speed = 20.0 * (1.609344 * (1.0 / 3.6));
        let rows = [
            ("come to stop", 5.0, vec![(-0.5, 12.0)]),
            ("start from stop", 0.0, vec![(1.5, 6.0)]),
            (
                "creep: alternate between +1m/s^2 and -1m/s^2",
                0.0,
                vec![
                    (1.0, 3.0),
                    (-1.0, 3.0),
                    (1.0, 3.0),
                    (-1.0, 3.0),
                    (1.0, 3.0),
                    (-1.0, 3.0),
                ],
            ),
            (
                "brake step response: -1m/s^2 from 20mph",
                step_speed,
                vec![(-1.0, 3.0)],
            ),
            (
                "brake step response: -4m/s^2 from 20mph",
                step_speed,
                vec![(-4.0, 3.0)],
            ),
            (
                "gas step response: +1m/s^2 from 20mph",
                step_speed,
                vec![(1.0, 3.0)],
            ),
            (
                "gas step response: +4m/s^2 from 20mph",
                step_speed,
                vec![(4.0, 3.0)],
            ),
        ];
        let mut presets = Vec::with_capacity(rows.len());
        for (description, speed, actions) in rows {
            presets.push(Preset {
                description,
                owner: Longitudinal::new(Sequence::new(
                    actions
                        .into_iter()
                        .map(|(accel, time)| Action {
                            accel: vec![accel],
                            time: vec![time],
                        })
                        .collect(),
                    2,
                    speed,
                )?),
            });
        }
        Ok(Self {
            stopping_speed,
            current: 0,
            presets,
        })
    }

    pub fn step(&mut self, input: &Input) -> Result<Command, Error> {
        let speed = maximum(input.speed, 0.0);
        let mut command = if let Some(preset) = self.presets.get_mut(self.current) {
            let acceleration = preset.owner.update(LongitudinalInput {
                speed,
                active: input.active,
                standstill: input.standstill,
                cruise_standstill: input.cruise_standstill,
            })?;
            let state = &preset.owner.sequence.state;
            let text = if state.active {
                format!("Maneuver Active: {acceleration:0.2} m/s^2")
            } else {
                let mph = preset.owner.sequence.initial_speed * (3.6 * (1.0 / 1.609344));
                format!("Setting up to {mph:0.2} mph")
            };
            let command = Command {
                acceleration,
                should_stop: false,
                alert_text1: text,
                alert_text2: preset.description,
                selected: Some(self.current),
                state: Some(state.clone()),
            };
            if state.finished {
                self.current = self.current.saturating_add(1);
            }
            command
        } else {
            Command {
                acceleration: 0.0,
                should_stop: false,
                alert_text1: "Maneuvers Finished".into(),
                alert_text2: "",
                selected: None,
                state: None,
            }
        };
        command.should_stop = speed < self.stopping_speed && command.acceleration < 1e-2;
        Ok(command)
    }
}
