use openpilot_logmessaged::JsonValue;
use openpilot_xiaoge::compute::{ComparisonCadence, ComputeMode, Difference, Selection};

#[test]
fn gpu_is_explicit_and_malformed_persistent_modes_return_to_cpu() {
    for raw in [
        None,
        Some(b"".as_slice()),
        Some(b"3".as_slice()),
        Some(b"-1".as_slice()),
        Some(b"1.0".as_slice()),
        Some(b"true".as_slice()),
        Some(b"\xff".as_slice()),
    ] {
        assert_eq!(ComputeMode::from_parameter(raw), ComputeMode::Cpu);
    }
    assert_eq!(ComputeMode::from_parameter(Some(b"1\n")), ComputeMode::Gpu);
    assert_eq!(
        ComputeMode::from_parameter(Some(b"2")),
        ComputeMode::Compare
    );
    for (raw, wanted) in [
        ("0", ComputeMode::Cpu),
        ("1", ComputeMode::Gpu),
        ("2", ComputeMode::Compare),
    ] {
        assert_eq!(
            ComputeMode::from_json(&JsonValue::parse(raw).unwrap()).unwrap(),
            wanted
        );
    }
    for raw in ["true", "\"1\"", "1.5", "-1", "3", "null"] {
        assert!(
            ComputeMode::from_json(&JsonValue::parse(raw).unwrap()).is_err(),
            "{raw}"
        );
    }
}

#[test]
fn failed_gpu_does_not_retry_until_selection_changes() {
    let mut state = Selection::default();
    assert!(!state.wants_gpu());
    assert!(state.select(ComputeMode::Gpu, 1));
    assert!(state.wants_gpu());
    state.fail_gpu();
    for _ in 0..100 {
        assert!(!state.select(ComputeMode::Gpu, 1));
        assert!(!state.wants_gpu());
    }
    assert!(state.select(ComputeMode::Cpu, 2));
    assert!(!state.wants_gpu());
    assert!(state.select(ComputeMode::Gpu, 3));
    assert!(state.wants_gpu());
}

#[test]
fn only_diagnostic_mode_reserves_a_shadow_sample_and_limits_its_cost() {
    let mut cadence = ComparisonCadence::default();
    assert!(!cadence.due(ComputeMode::Cpu, 100.0));
    assert!(!cadence.due(ComputeMode::Gpu, 100.0));
    assert!(cadence.due(ComputeMode::Compare, 100.0));
    assert!(!cadence.due(ComputeMode::Compare, 100.1));
    assert!(!cadence.due(ComputeMode::Compare, 109.999));
    assert!(!cadence.due(ComputeMode::Compare, 99.0));
    assert!(!cadence.due(ComputeMode::Compare, f64::NAN));
    assert!(cadence.due(ComputeMode::Compare, 110.0));
    assert!(cadence.due(ComputeMode::Compare, 1000.0));
    assert!(!cadence.due(ComputeMode::Compare, 1000.0));
}

#[test]
fn comparison_exposes_nonfinite_values_shape_errors_and_real_numeric_deltas() {
    let equal = Difference::between(&[0., 1., -3.], &[0., 1., -3.]).unwrap();
    assert_eq!(equal.elements, 3);
    assert_eq!(equal.unequal_bits, 0);
    assert_eq!(equal.max_abs, 0.0);
    let changed = Difference::between(&[1., 2.], &[1.5, 1.]).unwrap();
    assert_eq!(changed.max_abs, 1.0);
    assert_eq!(changed.mean_abs, 0.75);
    assert_eq!(changed.max_rel, 0.5);
    assert_eq!(changed.unequal_bits, 2);
    let invalid = Difference::between(&[f32::NAN, f32::INFINITY, 1.], &[f32::NAN, 0., 2.]).unwrap();
    assert_eq!(invalid.nonfinite, 2);
    assert!(Difference::between(&[1.], &[1., 2.]).is_err());
}

#[test]
fn changing_mode_expires_prior_results_and_preserves_detection_settings() {
    use openpilot_xiaoge::{
        config::Config,
        service::{Model, State},
    };
    let mut state = State::new(
        Config::default(),
        ["bsd", "lane"].map(|path| Model {
            path: path.into(),
            loaded: true,
            error: String::new(),
        }),
        1.0,
    );
    let settings = serde_json::to_value(state.settings).unwrap();
    state.lane.valid = true;
    state.lane_updated = 42;
    state.blindspot_updated = 43;
    state.blindspot_side_updated = [42, 43];
    state.blindspot_active = [true; 2];
    state.select_compute(ComputeMode::Gpu).unwrap();
    assert_eq!(state.compute_generation, 1);
    assert!(!state.lane.valid);
    assert_eq!(state.lane_updated, 0);
    assert_eq!(state.blindspot_side_updated, [0, 0]);
    assert_eq!(state.blindspot_active, [false, false]);
    assert_eq!(serde_json::to_value(state.settings).unwrap(), settings);
    state.select_compute(ComputeMode::Gpu).unwrap();
    assert_eq!(state.compute_generation, 1);
    state.select_compute(ComputeMode::Cpu).unwrap();
    assert_eq!(state.compute_generation, 2);
}

#[test]
fn a_new_generation_resets_the_latch_even_when_intermediate_modes_were_not_observed() {
    let mut state = Selection::default();
    assert!(state.select(ComputeMode::Gpu, 1));
    state.fail_gpu();
    assert!(!state.select(ComputeMode::Gpu, 1));
    assert!(!state.wants_gpu());
    // The owner selected CPU and then GPU while the worker was busy.
    assert!(state.select(ComputeMode::Gpu, 3));
    assert!(state.wants_gpu());
    assert!(state.select(ComputeMode::Cpu, 4));
    assert!(state.select(ComputeMode::Cpu, 6));
    assert!(!state.wants_gpu());
}
