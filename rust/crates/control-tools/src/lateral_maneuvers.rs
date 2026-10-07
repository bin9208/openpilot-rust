//! Port of openpilot/tools/lateral_maneuvers/lateral_maneuversd.py; original MIT license applies.
use crate::{
    maneuver::{Action, Lateral, LateralInput, Sequence, State, DT},
    Error,
};
use openpilot_control_policy::math::maximum;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Input {
    pub speed: f64,
    pub active: bool,
    pub steering_pressed: bool,
    pub curvature: f64,
    pub orientation: Vec<f64>,
    pub valid: bool,
}

pub struct Command {
    pub acceleration: f64,
    pub valid: bool,
    pub curvature: f64,
    pub alert_text1: String,
    pub alert_text2: Option<&'static str>,
    pub selected: Option<usize>,
    pub state: Option<State>,
    pub baseline: f64,
    pub complete_remaining: u32,
    pub display_holdoff: u32,
}

struct Preset {
    description: &'static str,
    owner: Lateral,
}

pub struct Controller {
    presets: Vec<Preset>,
    current: usize,
    complete_remaining: u32,
    display_holdoff: u32,
    previous_text: String,
}

fn sine() -> Action {
    let time: Vec<_> = (0_u32..=40)
        .map(|index| {
            if index == 40 {
                2.0
            } else {
                f64::from(index) * (2.0 / 40.0)
            }
        })
        .collect();
    let accel = time
        .iter()
        .map(|time| (2.0 * std::f64::consts::PI * time / 2.0).sin())
        .collect();
    Action { accel, time }
}

impl Controller {
    pub fn new() -> Result<Self, Error> {
        let mut presets = Vec::with_capacity(6);
        for (mph, descriptions) in [
            (
                20.0,
                ["step right 20mph", "step left 20mph", "sine 0.5Hz 20mph"],
            ),
            (
                30.0,
                ["step right 30mph", "step left 30mph", "sine 0.5Hz 30mph"],
            ),
        ] {
            for (index, description) in descriptions.into_iter().enumerate() {
                let actions = match index {
                    0 => vec![
                        Action {
                            accel: vec![0.5],
                            time: vec![1.0],
                        },
                        Action {
                            accel: vec![-0.5],
                            time: vec![1.5],
                        },
                    ],
                    1 => vec![
                        Action {
                            accel: vec![-0.5],
                            time: vec![1.0],
                        },
                        Action {
                            accel: vec![0.5],
                            time: vec![1.5],
                        },
                    ],
                    2 => vec![
                        sine(),
                        Action {
                            accel: vec![0.0],
                            time: vec![0.5],
                        },
                    ],
                    _ => return Err(Error::Contract("lateral preset index")),
                };
                let initial_speed = mph * (1.609344 * (1.0 / 3.6));
                presets.push(Preset {
                    description,
                    owner: Lateral::new(Sequence::new(actions, 2, initial_speed)?),
                });
            }
        }
        Ok(Self {
            presets,
            current: 0,
            complete_remaining: 0,
            display_holdoff: 0,
            previous_text: String::new(),
        })
    }

    pub fn step(&mut self, input: &Input) -> Result<Command, Error> {
        let speed = maximum(input.speed, 0.0);
        let mut acceleration = 0.0;
        let mut text2 = None;
        let mut text = if let Some(preset) = self.presets.get_mut(self.current) {
            let owner = &mut preset.owner;
            if self.complete_remaining > 0 {
                self.complete_remaining -= 1;
                text2 = Some(preset.description);
                "Completed".into()
            } else {
                if input.steering_pressed
                    || (owner.sequence.state.active
                        && (speed - owner.sequence.initial_speed).abs() > 0.7)
                {
                    owner.reset();
                }
                let roll = if input.orientation.len() == 3 {
                    input.orientation[0]
                } else {
                    0.0
                };
                acceleration = owner.update(LateralInput {
                    speed,
                    active: input.active,
                    curvature: input.curvature,
                    roll,
                })?;
                if owner.sequence.state.run_completed {
                    self.complete_remaining = 20;
                    text2 = Some(preset.description);
                    "Complete".into()
                } else if owner.sequence.state.active {
                    let remaining = maximum(owner.sequence.action_remaining()?, 0.0);
                    text2 = Some(preset.description);
                    if preset.description.starts_with("sine") {
                        format!("Active sine 0.5Hz {remaining:.1}s")
                    } else {
                        format!("Active {acceleration:+.1}m/s짼 {remaining:.1}s")
                    }
                } else if !((speed - owner.sequence.initial_speed).abs() < 0.7 && input.active) {
                    let mph = owner.sequence.initial_speed * (3.6 * (1.0 / 1.609344));
                    format!("Set speed to {mph:0.0} mph")
                } else if owner.sequence.state.ready_count > 0 {
                    let count = u32::try_from(owner.sequence.state.ready_count)
                        .map_err(|_| Error::Contract("inactive readiness counter"))?;
                    let time = maximum(2.0 - f64::from(count) * DT, 0.0);
                    text2 = Some(preset.description);
                    format!("Starting: {}", if time >= 1.0 { 2 } else { 1 })
                } else {
                    text2 = Some(preset.description);
                    format!(
                        "Waiting: {}",
                        if input.curvature.abs() < 0.002 {
                            "road not flat"
                        } else {
                            "road not straight"
                        }
                    )
                }
            }
        } else {
            "Maneuvers Finished".into()
        };
        let setup = |text: &str| {
            ["Set speed", "Starting", "Waiting"]
                .iter()
                .any(|prefix| text.starts_with(prefix))
        };
        let same = text == self.previous_text
            || (text.starts_with("Starting") && self.previous_text.starts_with("Starting"));
        if !same && setup(&text) && setup(&self.previous_text) && self.display_holdoff > 0 {
            text.clone_from(&self.previous_text);
            self.display_holdoff -= 1;
        } else {
            self.previous_text.clone_from(&text);
            self.display_holdoff = if setup(&text) { 10 } else { 0 };
        }
        let (selected, state, baseline, valid) = match self.presets.get(self.current) {
            Some(preset) => (
                Some(self.current),
                Some(preset.owner.sequence.state.clone()),
                preset.owner.baseline_curvature,
                preset.owner.sequence.state.active && self.complete_remaining == 0,
            ),
            None => (None, None, 0.0, false),
        };
        let curvature = if valid {
            baseline + acceleration / maximum(speed, 1.0).powi(2)
        } else {
            0.0
        };
        if state.as_ref().is_some_and(|state| state.finished) && self.complete_remaining == 0 {
            self.current = self.current.saturating_add(1);
        }
        Ok(Command {
            acceleration,
            valid,
            curvature,
            alert_text1: text,
            alert_text2: text2,
            selected,
            state,
            baseline,
            complete_remaining: self.complete_remaining,
            display_holdoff: self.display_holdoff,
        })
    }
}
