//! Independent controller for the original seven owned-PortAudio soundd checks.
//! No soundd policy or generated fixture is used to construct expected answers.
mod cli;
mod identity;
mod worker;
use crate::seal::Result;
pub use cli::{parse, Command, Options};
pub use identity::validate_source;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Live,
    Bounded,
    Retry,
    Inactive,
    StartFailed,
    StopRetry,
    Exhausted,
}
impl Mode {
    pub const ALL: [Self; 7] = [
        Self::Live,
        Self::Bounded,
        Self::Retry,
        Self::Inactive,
        Self::StartFailed,
        Self::StopRetry,
        Self::Exhausted,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Bounded => "bounded",
            Self::Retry => "retry",
            Self::Inactive => "inactive",
            Self::StartFailed => "start-failed",
            Self::StopRetry => "stop-retry",
            Self::Exhausted => "exhausted",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|m| m.as_str() == s)
            .ok_or_else(|| format!("invalid soundd mode: {s}").into())
    }
}
#[derive(Clone, Copy, Debug)]
pub enum SampleStage {
    Engage,
    Language,
    MainHeld,
    EnabledTimeout,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Event {
    pub event: String,
    pub value: i64,
    pub time: f64,
    pub pid: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Metrics {
    pub opens: usize,
    pub callbacks: usize,
    pub median_period: Option<f64>,
}
pub(super) fn require(ok: bool, why: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(why.into().into())
    }
}
pub fn validate_events(mode: Mode, pid: u32, code: i32, events: &[Event]) -> Result<Metrics> {
    require(
        code == if matches!(mode, Mode::Exhausted | Mode::Inactive | Mode::StartFailed) {
            1
        } else {
            0
        },
        format!("{}: daemon exit {code}", mode.as_str()),
    )?;
    require(
        !events.is_empty() && events.iter().all(|e| e.pid == pid),
        "PortAudio trace does not belong exclusively to the actual daemon",
    )?;
    let opens: Vec<_> = events.iter().filter(|e| e.event == "open").collect();
    require(
        opens.len()
            == match mode {
                Mode::Retry => 3,
                Mode::Exhausted => 10,
                _ => 1,
            },
        "PortAudio open count mismatch",
    )?;
    if mode == Mode::Retry {
        require(
            opens.windows(2).all(|w| {
                let d = w[1].time - w[0].time;
                2.95 < d && d < 4.0
            }),
            "PortAudio retry interval outside original strict bounds",
        )?;
    }
    if !matches!(mode, Mode::StopRetry | Mode::Exhausted) {
        require(
            events
                .iter()
                .rev()
                .take(3)
                .map(|e| e.event.as_str())
                .collect::<Vec<_>>()
                == ["terminate", "close", "stop"],
            "PortAudio stop/close/terminate ordering mismatch",
        )?;
    }
    require(
        events.last().is_some_and(|e| e.event == "terminate"),
        "missing PortAudio termination",
    )?;
    let polls: Vec<_> = events
        .iter()
        .filter(|e| e.event == "poll")
        .map(|e| e.time)
        .collect();
    let median_period = if polls.len() > 2 {
        let mut d: Vec<_> = polls.windows(2).map(|w| w[1] - w[0]).collect();
        // numpy.median propagates NaN; isolated infinities need not change a
        // finite median. Do not impose a new all-finite input policy.
        require(d.iter().all(|x| !x.is_nan()), "NaN poll interval")?;
        d.sort_by(f64::total_cmp);
        let n = d.len();
        let median = if n.is_multiple_of(2) {
            (d[n / 2 - 1] + d[n / 2]) / 2.0
        } else {
            d[n / 2]
        };
        require(
            0.04 < median && median < 0.065,
            format!("poll median outside original strict bounds: {median}"),
        )?;
        Some(median)
    } else {
        None
    };
    Ok(Metrics {
        opens: opens.len(),
        callbacks: events.iter().filter(|e| e.event == "callback").count(),
        median_period,
    })
}
/// Original NumPy float32 predicates, including -0 equality and ignored partial floats.
pub fn validate_samples(stage: SampleStage, bytes: &[u8]) -> Result<()> {
    let words = bytes.len() / 4;
    require(
        words.is_multiple_of(4096),
        "samples cannot reshape to 4096-column buffers",
    )?;
    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().expect("four bytes")))
        .collect();
    let first = |bits| {
        floats
            .chunks_exact(4096)
            .any(|row| row[0] == f32::from_bits(bits))
    };
    match stage {
        SampleStage::Engage => require(first(1008865116), "engage waveform missing"),
        SampleStage::Language => require(first(1016201216), "language waveform missing"),
        SampleStage::MainHeld => {
            require(
                floats.iter().take(20 * 4096).all(|&v| v == 0.0),
                "MAIN fired before two seconds",
            )?;
            require(first(1016201216), "MAIN did not produce prompt")
        }
        SampleStage::EnabledTimeout => {
            require(
                floats
                    .iter()
                    .skip(2 * 4096)
                    .take(53 * 4096)
                    .all(|&v| v == 0.0),
                "timeout fired before five seconds",
            )?;
            require(
                floats
                    .chunks_exact(4096)
                    .skip(61)
                    .any(|row| row[0] == f32::from_bits(1020985344)),
                "timeout warning did not play",
            )
        }
    }
}
pub fn queue_capacity(topic: &str) -> Result<usize> {
    match topic {
        "selfdriveState" | "carrotMan" | "soundPressure" | "carState" => Ok(256000),
        _ => Err(format!("unsupported soundd topic: {topic}").into()),
    }
}
pub fn input_packet(topic: &str, values: &Value, now: u64) -> Result<Vec<u8>> {
    use openpilot_cereal::{car_capnp::car_control::h_u_d_control::AudibleAlert, log_capnp::event};
    let mut message = capnp::message::Builder::new_default();
    let mut e = message.init_root::<event::Builder>();
    e.set_log_mono_time(now);
    e.set_valid(false);
    match topic {
        "soundPressure" => {
            e.init_sound_pressure().set_sound_pressure_weighted_db(
                values["soundPressureWeightedDb"]
                    .as_f64()
                    .ok_or("missing sound pressure")? as f32,
            );
        }
        "carrotMan" => {
            e.init_carrot_man().set_left_sec(i32::try_from(
                values["leftSec"].as_i64().ok_or("missing leftSec")?,
            )?);
        }
        "selfdriveState" => {
            let mut s = e.init_selfdrive_state();
            s.set_enabled(values["enabled"].as_bool().ok_or("missing enabled")?);
            s.set_alert_sound(match values["alertSound"].as_u64() {
                Some(0) => AudibleAlert::None,
                Some(1) => AudibleAlert::Engage,
                Some(7) => AudibleAlert::PromptRepeat,
                _ => return Err("unsupported soundd alert input".into()),
            });
        }
        "carState" => {
            let values = values["buttonEvents"]
                .as_array()
                .ok_or("missing buttonEvents")?;
            let mut buttons = e
                .init_car_state()
                .init_button_events(u32::try_from(values.len())?);
            for (i, v) in values.iter().enumerate() {
                require(v["type"] == "mainCruise", "unsupported button type")?;
                let mut b = buttons.reborrow().get(i as u32);
                b.set_type(openpilot_cereal::car_capnp::car_state::button_event::Type::MainCruise);
                b.set_pressed(v["pressed"].as_bool().ok_or("missing pressed")?);
            }
        }
        _ => return Err(format!("unsupported soundd packet: {topic}").into()),
    }
    Ok(capnp::serialize::write_message_to_words(&message))
}
/// Original root summary array; actual processes run only in the private worker.
pub fn run(options: &Options, worker: &Path) -> Result<Value> {
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = worker::run(options, worker, &cancel)?;
    worker::finish_cli(&result, &cancel)?;
    Ok(result.summary)
}
pub fn cli() -> Result<()> {
    use std::{
        io::Write,
        sync::{atomic::AtomicBool, Arc},
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|s| s == "--soundd-daemon-worker") {
        return worker::dispatch(&args[1..]);
    }
    match parse(
        &args,
        &std::env::current_dir()?,
        std::env::vars_os().collect(),
    )? {
        Command::Help => {
            io::stdout().lock().write_all(cli::HELP.as_bytes())?;
            Ok(())
        }
        Command::Run(options) => {
            let cancel = Arc::new(AtomicBool::new(false));
            let mut handlers = Vec::new();
            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                handlers.push(signal_hook::flag::register(signal, Arc::clone(&cancel))?);
            }
            let result =
                worker::run(&options, &std::env::current_exe()?, &cancel).and_then(|result| {
                    worker::finish_cli(&result, &cancel)?;
                    Ok(result)
                });
            // Publication observes the latch before signal handlers are removed.
            for handler in handlers {
                signal_hook::low_level::unregister(handler);
            }
            // Retain the publication descriptor and observe any signal latched
            // while handlers were being removed, before releasing ownership.
            result.and_then(|result| worker::finish_cli(&result, &cancel))
        }
    }
}
