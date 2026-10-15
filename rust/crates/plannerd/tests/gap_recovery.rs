use openpilot_plannerd::gap_recovery::{GapSample, LeadGapState};

fn opening() -> GapSample {
    GapSample {
        level: 2,
        track_id: 8,
        enabled: true,
        dt: 0.05,
        ego_speed: 20.,
        lead_speed: 22.,
        relative_speed: 2.,
        distance: 40.,
        desired_distance: 20.,
        base_tf: 1.45,
        stop_distance: 6.,
    }
}

#[test]
fn unchanged_opening_gap_is_captured_once() {
    // Given: a measured opening gap with half a second of capturable headroom.
    let mut state = LeadGapState::default();
    let input = opening();
    let mut margin = 0.;
    // When: the same gap persists through acquisition and steady-state updates.
    for _ in 0..100 {
        margin = state.update(input);
    }
    // Then: repeated updates do not accumulate the same gap again.
    assert_eq!(margin, 0.5);
}

#[test]
fn maximum_response_level_clears_extra_headroom() {
    // Given: a captured opening gap at an ordinary response level.
    let mut state = LeadGapState::default();
    for _ in 0..20 {
        state.update(opening());
    }
    let mut input = opening();
    input.level = 5;
    // When: maximum-response mode explicitly disables comfort headroom.
    let margin = state.update(input);
    // Then: no previously captured extra TF survives.
    assert_eq!(margin, 0.);
}

#[test]
fn opening_gap_outside_distance_band_releases_even_for_stopped_lead() {
    let mut owner = LeadGapState::default();
    let mut frame = opening();
    frame.lead_speed = 0.;
    frame.distance = 70.;
    let mut first = 0.;
    let mut last = 0.;
    for index in 0..400 {
        last = owner.update(frame);
        if index == 15 {
            first = last;
        }
    }
    assert!(first > 0.1);
    assert!(last < first * 0.001, "{first} -> {last}");
}
