use crate::Error;
use num_traits::ToPrimitive;
use openpilot_runtime_core::filters::FirstOrderFilter;
use serde::{Deserialize, Serialize};

pub struct Sound {
    pub alert: u16,
    pub samples: Vec<f32>,
    pub loops: Option<usize>,
}
pub struct Playback {
    pub sounds: Vec<Sound>,
    pub alert: u16,
    pub frame: usize,
    pub volume: f64,
}
impl Playback {
    pub fn new(sounds: Vec<Sound>) -> Self {
        Self {
            sounds,
            alert: 0,
            frame: 0,
            volume: 0.1,
        }
    }
    pub fn update_alert(&mut self, alert: u16) -> Option<u16> {
        let unsupported = alert != 0 && !self.sounds.iter().any(|sound| sound.alert == alert);
        let new_alert = if unsupported { 0 } else { alert };
        let played = self.alert == 0
            || self
                .sounds
                .iter()
                .find(|sound| sound.alert == self.alert)
                .is_none_or(|sound| self.frame >= sound.samples.len());
        if self.alert != new_alert && (new_alert != 0 || played) {
            self.alert = new_alert;
            self.frame = 0;
        }
        unsupported.then_some(alert)
    }
    pub fn render(&mut self, output: &mut [f32]) -> Result<(), Error> {
        output.fill(0.);
        if self.alert != 0 {
            let sound = self
                .sounds
                .iter()
                .find(|sound| sound.alert == self.alert)
                .ok_or(Error::Contract("active sound disappeared"))?;
            if sound.samples.is_empty() {
                return Err(Error::Contract("empty active sound"));
            }
            let offset = self.frame % sound.samples.len();
            let loops = self.frame / sound.samples.len();
            // Source deliberately keeps callback-local offset and loops unchanged.
            if sound.loops.is_none_or(|limit| loops < limit) {
                for chunk in output.chunks_mut(sound.samples.len() - offset) {
                    chunk.copy_from_slice(&sound.samples[offset..offset + chunk.len()]);
                    self.frame = self
                        .frame
                        .checked_add(chunk.len())
                        .ok_or(Error::Contract("sample counter overflow"))?;
                }
            }
        }
        // NumPy multiplies the float32 array by its scalar cast to float32.
        let volume = self
            .volume
            .to_f32()
            .ok_or(Error::Contract("volume conversion"))?;
        for sample in output {
            *sample *= volume;
        }
        Ok(())
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Input {
    pub now: f64,
    pub updated_selfdrive: bool,
    pub updated_carrot: bool,
    pub selfdrive_received: f64,
    pub enabled: bool,
    pub alert: u16,
    pub countdown: i32,
    pub pressure: Option<f64>,
    pub main_buttons: Vec<bool>,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub alert: u16,
    pub frame: usize,
    pub volume: f64,
    pub adjust: f64,
    pub countdown: i32,
    pub timeout: bool,
    pub filter: f64,
}
pub struct Policy {
    pub playback: Playback,
    pub adjust: f64,
    pub countdown: i32,
    pub timeout: bool,
    filter: FirstOrderFilter,
    pressed_at: Option<f64>,
    triggered: bool,
    tizi: bool,
}
impl Policy {
    pub fn new(sounds: Vec<Sound>, tizi: bool) -> Self {
        Self {
            playback: Playback::new(sounds),
            adjust: 1.,
            countdown: 0,
            timeout: false,
            filter: FirstOrderFilter::new(0., 2.5, 0.1, false),
            pressed_at: None,
            triggered: false,
            tizi,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            alert: self.playback.alert,
            frame: self.playback.frame,
            volume: self.playback.volume,
            adjust: self.adjust,
            countdown: self.countdown,
            timeout: self.timeout,
            filter: self.filter.value(),
        }
    }
    pub fn step(&mut self, input: &Input) -> Option<u16> {
        if let Some(db) = input.pressure.filter(|_| self.playback.alert == 0) {
            let filtered = self.filter.update(db);
            let ambient = if self.tizi { 30. } else { 24. };
            let base: f64 = if self.tizi { 10. } else { 20. };
            let volume = ((filtered - ambient) / 30.) * 0.9 + 0.1;
            self.playback.volume = base.powf(volume.clamp(0.1, 1.) - 1.) * self.adjust;
        }
        for pressed in &input.main_buttons {
            if *pressed {
                if self.pressed_at.is_none() {
                    self.pressed_at = Some(input.now);
                    self.triggered = false;
                }
            } else {
                self.pressed_at = None;
                self.triggered = false;
            }
        }
        if self.pressed_at.is_some_and(|at| input.now - at >= 2.)
            && !self.triggered
            && !input.enabled
        {
            self.triggered = true;
            return self.playback.update_alert(6);
        }
        if input.updated_selfdrive || input.updated_carrot {
            let mut alert = input.alert;
            if alert == 0 && self.countdown != input.countdown {
                self.countdown = input.countdown;
                alert = match input.countdown {
                    0 => 11,
                    1..=10 => u16::try_from(input.countdown + 23).unwrap_or(0),
                    11 => 8,
                    _ => 0,
                };
            }
            return self.playback.update_alert(alert);
        }
        let missing = input.now - input.selfdrive_received;
        if missing > 5. && input.enabled && missing - 5. < 10. {
            self.playback.update_alert(5);
            self.timeout = true;
        } else if self.timeout {
            self.playback.update_alert(0);
            self.timeout = false;
        }
        None
    }
}
