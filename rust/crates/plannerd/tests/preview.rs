use openpilot_plannerd::{
    driving_mode::DrivingMode,
    preview::{apply_target, clip_offset, rate_limit, request, PreviewInput},
};

#[test]
fn lost_lead_releases_braking_preview_progressively() {
    // Given: accumulated preview and a source frame without a usable lead.
    let input = PreviewInput {
        lead_status: false,
        lead_acceleration: -2.,
        ego_acceleration: 0.,
    };
    let request = request(input);
    // When: applying the normal per-plan release limit.
    let output = rate_limit(request.offset_s, 0.20);
    // Then: the existing correction releases by 0.03 seconds.
    assert_eq!(output, 0.20 - 0.03);
}

#[test]
fn preview_never_adds_post_solver_acceleration() {
    // Given: preview suggests more acceleration than the base MPC target.
    let base = 0.25;
    // When: applying preview in every driving mode.
    for mode in [
        DrivingMode::Eco,
        DrivingMode::Safe,
        DrivingMode::Normal,
        DrivingMode::High,
    ] {
        let output = apply_target(base, 1., mode);
        // Then: the source's base acceleration remains the ceiling.
        assert_eq!(output, base);
    }
}

#[test]
fn effective_preview_respects_the_action_time_upper_bound() {
    // Given: actuator action already occurs near the source upper bound.
    let base = 2.4;
    // When: requesting a one-second preview.
    let output = clip_offset(base, 1.);
    // Then: only the effective remaining headroom is retained.
    assert_eq!(output, 2.5 - base);
}
