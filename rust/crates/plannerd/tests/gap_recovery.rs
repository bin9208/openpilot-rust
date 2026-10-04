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
        distance: 50.,
        desired_distance: 30.,
        base_tf: 1.45,
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
