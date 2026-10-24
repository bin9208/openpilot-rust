//! Independent source-derived soundd policy and #392 sample reference.
//!
//! This deliberately does not depend on openpilot-soundd or read expected
//! fixtures. Legacy playback is retained only to describe the unchanged source
//! outputs; corrected playback uses the reviewed per-sample linear contract.
use crate::seal::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

mod wave;

const SOURCES: &[(&str, &str)] = &[
    (
        "rust/tools/check_soundd.py",
        "3ec6eb74836e2dca07fbe4bc16e80a2f4d4c20af47bf49a9a4dec0e886411f9b",
    ),
    (
        "rust/tools/original_params_binding.py",
        "b5fa10da96aecca63bafef0c03f6ddf9fe5fde89683f10e70742b4e919c38457",
    ),
    (
        "openpilot/selfdrive/ui/soundd.py",
        "d8abf04e9c11eba35fa262c4659afc0f90104e9dc7f3a84e1b4edea5daf6a843",
    ),
    (
        "openpilot/common/filter_simple.py",
        "38d315a00635c4093cf6cbc3ed46a90d0f1708daf7662cba8a00d069c20dbfb3",
    ),
    (
        "openpilot/selfdrive/car/openpilot_toggle.py",
        "b59d7409efd08a94336e7f3f5085930492fabff7b3fbbf31b5b930482c7cf798",
    ),
    (
        "openpilot/common/utils.py",
        "b1cc68b63cff1386ba397c0cc527dbb38fb50b154043ffcb066ed7acce65ec83",
    ),
    (
        "openpilot/system/micd.py",
        "cfafc90503405f481f666618871a6a25f6305a8ca38cf87bcafc154c3a375164",
    ),
];

/// Historical original tool identity remains explicit even after retirement.
pub fn historical_sources() -> BTreeMap<String, String> {
    BTreeMap::from([(SOURCES[0].0.to_owned(), SOURCES[0].1.to_owned())])
}
pub fn sources(repository: &Path) -> Result<BTreeMap<String, String>> {
    let mut live = BTreeMap::new();
    for &(name, expected) in SOURCES {
        let path = repository.join(name);
        if name == SOURCES[0].0
            && fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            continue;
        }
        let hash = format!("{:x}", Sha256::digest(fs::read(&path)?));
        if hash != expected {
            return Err(format!("soundd policy source changed: {name}").into());
        }
        live.insert(name.to_owned(), hash);
    }
    Ok(live)
}

pub fn steps() -> Vec<Value> {
    let mut rows = Vec::new();
    fn add(rows: &mut Vec<Value>, alert: u16, frames: usize, changes: Value) {
        let mut input = json!({"now": rows.len() as f64 * 1.1,
            "updated_selfdrive": true, "updated_carrot": false,
            "selfdrive_received": 0.0, "enabled": false, "alert": alert,
            "countdown": -1, "pressure": null, "main_buttons": []});
        for (key, value) in changes.as_object().expect("owned changes").iter() {
            input[key] = value.clone();
        }
        rows.push(json!({"input":input,"frames":frames,"adjust":0.73,"language":null}));
    }
    add(&mut rows, 0, 11, json!({"pressure":24.0}));
    rows.last_mut().expect("first row")["adjust"] = json!(-0.73);
    add(&mut rows, 0, 11, json!({"pressure":30.0}));
    for alert in (1..38).chain(std::iter::once(65535)) {
        add(&mut rows, alert, 2, json!({}));
        add(&mut rows, alert, 17, json!({}));
        add(&mut rows, 0, 3, json!({}));
    }
    for count in [12, 11, 11, 10, 3, 1, 0, -1] {
        add(
            &mut rows,
            0,
            11,
            json!({"countdown":count,
            "updated_selfdrive":false,"updated_carrot":true}),
        );
    }
    add(&mut rows, 0, 11, json!({"pressure":54.0}));
    add(&mut rows, 0, 11, json!({"pressure":30.0}));
    for missing in [5.0, 5.000001, 14.999999, 15.0, 15.000001] {
        let received = rows.len() as f64 * 1.1 - missing;
        add(
            &mut rows,
            0,
            11,
            json!({"updated_selfdrive":false,
            "enabled":true,"selfdrive_received":received}),
        );
    }
    add(&mut rows, 0, 2, json!({"main_buttons":[true]}));
    add(&mut rows, 0, 2, json!({}));
    add(&mut rows, 0, 2, json!({}));
    add(&mut rows, 0, 11, json!({"main_buttons":[false]}));
    for language in [" ko_KR ", "main-zh-Hans", "en", "main-ko", "zh-TW", "zz"] {
        add(&mut rows, 1, 3, json!({}));
        rows.last_mut().expect("language row")["language"] = json!(language);
    }
    rows
}

pub fn create_assets(root: &Path, rate: u32, channels: u16) -> Result<PathBuf> {
    if rate == 0 || !matches!(channels, 1 | 2) {
        return Err("invalid owned WAV fixture parameters".into());
    }
    let base = root.join("openpilot/selfdrive/assets");
    fs::create_dir_all(&base)?;
    for (folder, values) in [
        ("sounds_eng", [1200_i16, -2400, 3600, -4800]),
        ("sounds", [800, -900, 1000, -1100]),
        ("sounds_chs", [-500, 600, -700, 800]),
    ] {
        let folder = base.join(folder);
        fs::create_dir(&folder)?;
        let mut pcm = Vec::new();
        for value in values {
            pcm.extend_from_slice(&value.to_le_bytes());
            if channels == 2 {
                pcm.extend_from_slice(&value.div_euclid(3).to_le_bytes());
            }
        }
        for name in [
            "prompt.wav",
            "engage.wav",
            "engage_tizi.wav",
            "disengage_tizi.wav",
        ] {
            wave::write(&folder.join(name), rate, channels, &pcm)?;
        }
    }
    Ok(base)
}

#[derive(Deserialize)]
struct Config {
    assets: PathBuf,
    language: String,
    engage_volume: f64,
    tizi: bool,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
struct Step {
    input: Input,
    frames: usize,
    adjust: f64,
    language: Option<String>,
}
#[derive(Deserialize)]
struct Input {
    now: f64,
    updated_selfdrive: bool,
    updated_carrot: bool,
    selfdrive_received: f64,
    enabled: bool,
    alert: u16,
    countdown: i32,
    pressure: Option<f64>,
    main_buttons: Vec<bool>,
}
struct Clip {
    samples: Vec<f32>,
    repeats: Option<usize>,
}
const CLIPS: &[(u16, &str, Option<usize>)] = &[
    (1, "engage.wav", Some(1)),
    (2, "disengage.wav", Some(1)),
    (3, "refuse.wav", Some(1)),
    (6, "prompt.wav", Some(1)),
    (7, "prompt.wav", None),
    (8, "prompt_distracted.wav", None),
    (4, "warning_soft.wav", None),
    (5, "warning_immediate.wav", None),
    (10, "tici_engaged.wav", None),
    (11, "tici_disengaged.wav", None),
    (12, "traffic_sign_green.wav", None),
    (13, "traffic_sign_changed.wav", None),
    (19, "audio_traffic_error.wav", None),
    (20, "audio_car_watchout.wav", None),
    (14, "audio_lane_change.wav", None),
    (22, "audio_stopstop.wav", None),
    (15, "audio_stopping.wav", None),
    (16, "audio_auto_hold.wav", None),
    (17, "audio_engage.wav", None),
    (18, "audio_disengage.wav", None),
    (21, "audio_speed_down.wav", None),
    (9, "audio_turn.wav", None),
    (23, "reverse_gear.wav", Some(1)),
    (24, "audio_1.wav", None),
    (25, "audio_2.wav", None),
    (26, "audio_3.wav", None),
    (27, "audio_4.wav", None),
    (28, "audio_5.wav", None),
    (29, "audio_6.wav", None),
    (30, "audio_7.wav", None),
    (31, "audio_8.wav", None),
    (32, "audio_9.wav", None),
    (33, "audio_10.wav", None),
    (35, "prompt.wav", Some(1)),
];
fn folder(language: &str) -> &'static str {
    let language = language.trim().replace('_', "-").to_lowercase();
    let language = language.strip_prefix("main-").unwrap_or(&language);
    if language == "ko" || language.starts_with("ko-") {
        "sounds"
    } else if language.starts_with("zh") {
        "sounds_chs"
    } else {
        "sounds_eng"
    }
}
fn load(config: &Config, language: &str) -> Result<BTreeMap<u16, Clip>> {
    let preferred = config.assets.join(folder(language));
    let fallback = config.assets.join("sounds_eng");
    CLIPS
        .iter()
        .map(|&(id, name, repeats)| {
            let name = match (config.tizi, id) {
                (true, 1) => "engage_tizi.wav",
                (true, 2) => "disengage_tizi.wav",
                _ => name,
            };
            let path = [
                preferred.join(name),
                fallback.join(name),
                fallback.join("prompt.wav"),
            ]
            .into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| format!("soundd missing assets: {name}"))?;
            // The original source loads this value through Params' integer field.
            let gain = if matches!(id, 1 | 2 | 23) {
                (config.engage_volume * 100.0).round_ties_even() / 100.0
            } else {
                1.0
            };
            Ok((
                id,
                Clip {
                    samples: wave::read(&path, gain)?,
                    repeats,
                },
            ))
        })
        .collect()
}

struct State {
    clips: BTreeMap<u16, Clip>,
    alert: u16,
    frame: usize,
    volume: f64,
    adjust: f64,
    countdown: i32,
    timeout: bool,
    filter: f64,
    initialized: bool,
    pressed: Option<f64>,
    triggered: bool,
}
impl State {
    fn update_alert(&mut self, requested: u16) {
        let next = if requested == 0 || self.clips.contains_key(&requested) {
            requested
        } else {
            0
        };
        let played = self.alert == 0
            || self
                .clips
                .get(&self.alert)
                .is_none_or(|clip| self.frame >= clip.samples.len());
        if self.alert != next && (next != 0 || played) {
            self.alert = next;
            self.frame = 0;
        }
    }
    fn step(&mut self, input: &Input, tizi: bool) {
        if let Some(db) = input.pressure.filter(|_| self.alert == 0) {
            if self.initialized {
                let alpha = 0.1 / (2.5 + 0.1);
                self.filter = (1.0 - alpha) * self.filter + alpha * db;
            } else {
                self.filter = db;
                self.initialized = true;
            }
            let (ambient, base): (f64, f64) = if tizi { (30.0, 10.0) } else { (24.0, 20.0) };
            let linear = ((self.filter - ambient) / 30.0) * 0.9 + 0.1;
            self.volume = base.powf(linear.clamp(0.1, 1.0) - 1.0) * self.adjust;
        }
        for pressed in &input.main_buttons {
            if *pressed {
                if self.pressed.is_none() {
                    self.pressed = Some(input.now);
                    self.triggered = false;
                }
            } else {
                self.pressed = None;
                self.triggered = false;
            }
        }
        if self.pressed.is_some_and(|at| input.now - at >= 2.0) && !self.triggered && !input.enabled
        {
            self.triggered = true;
            self.update_alert(6);
        } else if input.updated_selfdrive || input.updated_carrot {
            let mut alert = input.alert;
            if alert == 0 && self.countdown != input.countdown {
                self.countdown = input.countdown;
                alert = match input.countdown {
                    0 => 11,
                    1..=10 => (input.countdown + 23) as u16,
                    11 => 8,
                    _ => 0,
                };
            }
            self.update_alert(alert);
        } else {
            let missing = input.now - input.selfdrive_received;
            if missing > 5.0 && input.enabled && missing - 5.0 < 10.0 {
                self.update_alert(5);
                self.timeout = true;
            } else if self.timeout {
                self.update_alert(0);
                self.timeout = false;
            }
        }
    }
    fn render(&mut self, count: usize, corrected: bool) -> Result<Vec<f32>> {
        let mut output = vec![0.0_f32; count];
        if self.alert != 0 {
            let clip = self.clips.get(&self.alert).ok_or("missing source alert")?;
            let n = clip.samples.len();
            if n == 0 {
                return Err("empty source sound".into());
            }
            let initial = self.frame;
            for (position, sample) in output.iter_mut().enumerate() {
                let index = if corrected { self.frame } else { initial };
                if clip.repeats.is_some_and(|limit| index / n >= limit) {
                    break;
                }
                let offset = if corrected {
                    index % n
                } else {
                    initial % n + position % (n - initial % n)
                };
                *sample = clip.samples[offset];
                self.frame = self
                    .frame
                    .checked_add(1)
                    .ok_or("source sample counter overflow")?;
            }
        }
        let gain = self.volume as f32;
        for value in &mut output {
            *value *= gain;
        }
        Ok(output)
    }
    fn snapshot(&self) -> Value {
        json!({"alert":self.alert,"frame":self.frame,"volume":self.volume,
            "adjust":self.adjust,"countdown":self.countdown,
            "timeout":self.timeout,"filter":self.filter})
    }
}

pub fn reference(repository: &Path, raw: &Value, corrected: bool) -> Result<Vec<Value>> {
    sources(repository)?;
    let config: Config = serde_json::from_value(raw.clone())?;
    let mut language = config.language.trim().to_owned();
    let mut next_language_check = 0.0;
    let mut state = State {
        clips: load(&config, &language)?,
        alert: 0,
        frame: 0,
        volume: 0.1,
        adjust: 1.0,
        countdown: 0,
        timeout: false,
        filter: 0.0,
        initialized: false,
        pressed: None,
        triggered: false,
    };
    let mut rows = Vec::new();
    for step in &config.steps {
        if let Some(candidate) = &step.language {
            if step.input.now >= next_language_check {
                next_language_check = step.input.now + 1.0;
                let candidate = candidate.trim();
                if language != candidate {
                    language = candidate.to_owned();
                    state.clips = load(&config, &language)?;
                    state.frame = 0;
                }
            }
        }
        state.step(&step.input, config.tizi);
        let samples = state.render(step.frames, corrected)?;
        state.adjust = step.adjust;
        rows.push(json!({"state":state.snapshot(),"samples":samples}));
    }
    Ok(rows)
}
