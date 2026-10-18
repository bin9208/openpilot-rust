//! #373: preserve source policy rejection before accessing sensor measurements.
//! Expectations come from unchanged original methods/main admission branch; no
//! native estimator supplied expected values. Numerical filter parity is separate.
#![cfg(feature = "solver")]

use openpilot_locationd::{estimator::Estimator, loop_state::LoopState, wire};
use openpilot_messaging::state::{Options, Poll, State};
use serde::Deserialize;
use serde_json::Value;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Corpus {
    issue: u32,
    source_sha256: String,
    direct: Vec<Direct>,
    #[serde(rename = "loop")]
    loops: Vec<Loop>,
    other_early_rejections: Vec<Direct>,
}
#[derive(Deserialize)]
struct Direct {
    name: String,
    packet: Vec<u8>,
    outcome: Outcome,
    logs: Vec<(String, String)>,
    observation_calls: Vec<Value>,
}
#[derive(Deserialize)]
struct Outcome {
    accepted: bool,
    result: Option<i32>,
}
#[derive(Deserialize)]
struct Loop {
    name: String,
    initialized_before: bool,
    packet: Vec<u8>,
    initialized_after: bool,
    invalid: [f64; 3],
    logs: Vec<(String, String)>,
    observation_calls: Vec<Value>,
}
fn corpus() -> Result<Corpus, serde_json::Error> {
    let value: Corpus = serde_json::from_slice(include_bytes!("data/admission-373/original.json"))?;
    assert_eq!(value.issue, 373);
    assert_eq!(value.direct.len(), 24);
    assert_eq!(value.loops.len(), 40);
    assert_eq!(value.other_early_rejections.len(), 4);
    assert_eq!(
        value.source_sha256,
        "aa29e3a4da21007bd93a99713f87c07b79407a52e067a1fdf82019c967103f5e"
    );
    Ok(value)
}
fn initialized_estimator() -> Result<Estimator, openpilot_locationd::Error> {
    let mut estimator = Estimator::new(true)?;
    estimator.kf.reset_default(Some(100.))?;
    Ok(estimator)
}

#[test]
fn original_stale_and_secondary_short_sensor_rejections_precede_vector_admission() -> TestResult {
    let mut failures = Vec::new();
    let mut checked = 0;
    for row in corpus()?.direct {
        if row.name.contains("-genuine-") {
            continue;
        }
        assert!(row.observation_calls.is_empty());
        assert!(row.outcome.accepted);
        let mut estimator = initialized_estimator()?;
        let actual = wire::decode(&row.packet)
            .and_then(|event| estimator.handle(event.log_time_ns as f64 * 1e-9, event.input));
        match actual {
            Ok(result)
                if Some(result as i32) == row.outcome.result && estimator.logs == row.logs => {}
            value => failures.push(format!(
                "{}: {value:?}, logs={:?}",
                row.name, estimator.logs
            )),
        }
        checked += 1;
    }
    assert_eq!(checked, 16);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[test]
fn genuine_accepted_sensor_measurements_keep_dimension_errors_and_valid_controls() -> TestResult {
    let mut checked = 0;
    for row in corpus()?.direct {
        if !row.name.contains("-genuine-") {
            continue;
        }
        let mut estimator = initialized_estimator()?;
        let actual = wire::decode(&row.packet)
            .and_then(|event| estimator.handle(event.log_time_ns as f64 * 1e-9, event.input));
        if row.outcome.accepted {
            assert_eq!(row.observation_calls.len(), 1, "{}", row.name);
            assert_eq!(actual? as i32, row.outcome.result.unwrap(), "{}", row.name);
        } else {
            assert!(row.observation_calls.is_empty());
            assert!(
                actual.is_err(),
                "{} must not invent missing vector components",
                row.name
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 8);
    Ok(())
}

#[test]
fn original_initialized_and_uninitialized_loop_sensor_admission_is_preserved() -> TestResult {
    let mut failures = Vec::new();
    let mut checked = 0;
    for row in corpus()?.loops {
        assert!(row.observation_calls.is_empty());
        let mut state = LoopState::new(true, true)?;
        state.initialized = row.initialized_before;
        state.estimator.kf.reset_default(Some(100.))?;
        let subscriber = State::new(
            &["carState", "liveCalibration", "cameraOdometry"],
            Options {
                simulation: true,
                poll: Poll::One("cameraOdometry".into()),
                ..Options::default()
            },
        )?;
        let outcome = wire::decode(&row.packet).and_then(|event| {
            if event.service == "accelerometer" {
                state.step(&subscriber, &[event], &[], || 100.)
            } else {
                state.step(&subscriber, &[], &[event], || 100.)
            }
        });
        let matches = match &outcome {
            Ok(None) => {
                state.initialized == row.initialized_after
                    && state.invalid == row.invalid
                    && state.estimator.logs == row.logs
            }
            _ => false,
        };
        if !matches {
            failures.push(format!(
                "{} initialized={}: error={:?}, actual_initialized={}, invalid={:?}, logs={:?}",
                row.name,
                row.initialized_before,
                outcome.err(),
                state.initialized,
                state.invalid,
                state.estimator.logs
            ));
        }
        checked += 1;
    }
    assert_eq!(checked, 40);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[derive(Deserialize)]
struct Followup {
    issue: u32,
    source_sha256: String,
    initialization: Vec<Initialization>,
    recovery: Vec<Recovery>,
}
#[derive(Deserialize)]
struct Initialization {
    acceleration: Vec<u8>,
    gyroscope: Vec<u8>,
    messages: Vec<Vec<u8>>,
    initialized: bool,
    sensor_valid: [bool; 2],
    sensor_alive: [bool; 2],
    sensor_received: [f64; 2],
    observation_calls: Vec<Value>,
}
#[derive(Deserialize)]
struct Recovery {
    service: String,
    steps: Vec<RecoveryStep>,
}
#[derive(Deserialize)]
struct RecoveryStep {
    packet: Vec<u8>,
    invalid: [f64; 3],
    logs: Vec<(String, String)>,
}
fn followup() -> Result<Followup, serde_json::Error> {
    let value: Followup =
        serde_json::from_slice(include_bytes!("data/admission-373/followup-original.json"))?;
    assert_eq!(value.issue, 373);
    assert_eq!(value.source_sha256, corpus()?.source_sha256);
    assert_eq!(value.initialization.len(), 6);
    assert_eq!(value.recovery.len(), 2);
    Ok(value)
}
fn subscriptions() -> Result<State, openpilot_messaging::state::Error> {
    State::new(
        &["carState", "liveCalibration", "cameraOdometry"],
        Options {
            simulation: true,
            poll: Poll::One("cameraOdometry".into()),
            ..Options::default()
        },
    )
}

#[test]
fn original_first_cycle_initialization_uses_validity_without_interpreting_axes() -> TestResult {
    for row in followup()?.initialization {
        assert!(row.observation_calls.is_empty());
        let mut state = LoopState::new(true, true)?;
        let mut subscriber = subscriptions()?;
        subscriber.update(100., &row.messages)?;
        let acceleration = wire::decode(&row.acceleration)?;
        let gyroscope = wire::decode(&row.gyroscope)?;
        let publication = state.step(&subscriber, &[acceleration], &[gyroscope], || 100.)?;
        assert!(publication.is_some());
        assert_eq!(state.initialized, row.initialized);
        assert_eq!(state.sensor_valid, row.sensor_valid);
        assert_eq!(state.sensor_alive, row.sensor_alive);
        assert_eq!(state.sensor_received, row.sensor_received);
        assert_eq!(state.invalid, [0.; 3]);
        assert!(state.estimator.logs.is_empty());
    }
    Ok(())
}

#[test]
fn original_rejected_input_counters_source_ignores_and_recovery_remain_unchanged() -> TestResult {
    for sequence in followup()?.recovery {
        let mut state = LoopState::new(true, true)?;
        state.initialized = true;
        state.estimator.kf.reset_default(Some(100.))?;
        let subscriber = subscriptions()?;
        assert_eq!(sequence.steps.len(), 5);
        for (index, step) in sequence.steps.into_iter().enumerate() {
            let event = wire::decode(&step.packet)?;
            let result = if sequence.service == "accelerometer" {
                state.step(&subscriber, &[event], &[], || 100.)?
            } else {
                state.step(&subscriber, &[], &[event], || 100.)?
            };
            assert!(result.is_none());
            assert_eq!(
                state.estimator.logs, step.logs,
                "{} step {index}",
                sequence.service
            );
            for (actual, original) in state.invalid.into_iter().zip(step.invalid) {
                assert!(
                    (actual - original).abs() <= 1e-12,
                    "{} step {index}: invalid {actual} != {original}",
                    sequence.service
                );
            }
            state.estimator.logs.clear();
        }
    }
    Ok(())
}
