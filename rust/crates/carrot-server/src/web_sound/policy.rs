use super::{toggle::MainToggle, Button, Settings};
use crate::Value;
use num_bigint::{BigInt, BigUint};

pub struct Input<'a> {
    pub now: f64,
    pub received: f64,
    pub enabled: bool,
    pub alert: u16,
    pub valid: bool,
    pub updated: bool,
    pub countdown: i32,
    pub countdown_valid: bool,
    pub countdown_updated: bool,
    pub car_valid: bool,
    pub buttons: &'a [Button],
    pub settings: Option<Settings>,
}

#[derive(Clone, PartialEq)]
struct Signature {
    alert: u16,
    countdown: i32,
    prompt_sequence: BigUint,
    settings: Settings,
    tizi: bool,
}

pub struct Policy {
    toggle: MainToggle,
    last: Option<Signature>,
    prompt_sequence: BigUint,
    sequence: BigUint,
    next_param_read: f64,
    settings: Settings,
    emitted_countdown: i32,
    tizi: bool,
}

impl Policy {
    pub fn new(tizi: bool) -> Self {
        Self {
            toggle: MainToggle::new(8),
            last: None,
            prompt_sequence: BigUint::default(),
            sequence: BigUint::default(),
            next_param_read: 0.,
            settings: Settings {
                volume: 1.,
                engage_volume: 1.,
                sound_directory: "sounds_eng",
            },
            emitted_countdown: 100,
            tizi,
        }
    }

    pub fn params_due(&self, now: f64) -> bool {
        now >= self.next_param_read
    }

    pub fn step(&mut self, input: Input<'_>) -> Option<Value> {
        let enabled = input.valid && input.enabled;
        let mut alert = if input.valid { input.alert } else { 0 };
        let missing = input.now - input.received;
        if missing > 5. {
            alert = if enabled && missing < 15. { 5 } else { 0 };
        }
        let countdown = if input.countdown_valid {
            input.countdown
        } else {
            100
        };
        if self.last.is_none() || input.updated || input.countdown_updated {
            self.emitted_countdown = countdown;
        }
        let buttons = if input.car_valid { input.buttons } else { &[] };
        if self.toggle.update(buttons, (enabled, input.now)) {
            self.prompt_sequence += 1_u8;
            alert = 6;
        }
        if self.params_due(input.now) {
            if let Some(settings) = input.settings {
                self.settings = settings;
            }
            self.next_param_read = input.now + 1.;
        }
        let signature = Signature {
            alert,
            countdown: self.emitted_countdown,
            prompt_sequence: self.prompt_sequence.clone(),
            settings: self.settings.clone(),
            tizi: self.tizi,
        };
        if self.last.as_ref() == Some(&signature) {
            return None;
        }
        self.sequence += 1_u8;
        let value = Value::object([
            ("type", Value::text("soundState")),
            (
                "sequence",
                Value::Integer(BigInt::from(self.sequence.clone())),
            ),
            ("alert", Value::integer(signature.alert)),
            ("countdown", Value::integer(signature.countdown)),
            (
                "promptSequence",
                Value::Integer(BigInt::from(signature.prompt_sequence.clone())),
            ),
            ("volume", Value::Float(signature.settings.volume)),
            (
                "engageVolume",
                Value::Float(signature.settings.engage_volume),
            ),
            (
                "soundDirectory",
                Value::text(signature.settings.sound_directory),
            ),
            ("tizi", Value::Bool(signature.tizi)),
        ]);
        self.last = Some(signature);
        Some(value)
    }
}
