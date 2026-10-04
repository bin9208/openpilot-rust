use openpilot_plannerd::coasting::{CoastingInput, CruiseCoastingPlan};

fn input() -> CoastingInput {
    CoastingInput {
        enabled: true,
        percent: 5.,
        set_speed: 21.,
        target: 20.,
        external_limit: 25.,
        dt: 0.05,
    }
}

#[test]
fn physical_reference_is_retained_after_stability() {
    // Given: a target stable across twenty source time steps.
    let mut owner = CruiseCoastingPlan::default();
    let mut frame = input();
    for _ in 0..20 {
        assert_eq!(owner.update(&frame), 0.);
    }
    // When: a sub-threshold target movement occurs at stability.
    frame.target = 20.01;
    let reference = owner.update(&frame);
    // Then: the initial physical target remains the reference.
    assert_eq!(reference, 20.);
}

#[test]
fn external_cap_inside_band_cancels_stability() {
    // Given: a stable overspeed band from 20 to 21 m/s.
    let mut owner = CruiseCoastingPlan::default();
    let mut frame = input();
    for _ in 0..21 {
        owner.update(&frame);
    }
    frame.external_limit = 21.;
    // When: an external cap touches the band's end.
    let reference = owner.update(&frame);
    // Then: relief stops and its stability state resets.
    assert_eq!(reference, 0.);
    assert_eq!(owner.stable_time(), 0.);
}

#[test]
fn invalid_time_resets_reference() {
    // Given: an owner with a previously accepted target.
    let mut owner = CruiseCoastingPlan::default();
    let mut frame = input();
    for _ in 0..21 {
        owner.update(&frame);
    }
    frame.dt = f64::NAN;
    // When: the source timing input is nonfinite.
    let reference = owner.update(&frame);
    // Then: the next usable frame must earn stability again.
    assert_eq!(reference, 0.);
    assert_eq!(owner.stable_time(), 0.);
}
