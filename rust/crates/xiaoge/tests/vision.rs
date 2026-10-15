use openpilot_xiaoge::nv12::{Frame, Layout};
use openpilot_xiaoge::vision::{fresh, gate, Blindspot, GateInput, Side};

#[test]
fn nv12_packing_excludes_scanline_padding_and_uv_gap() {
    // Given padded Y/UV planes with a separate allocation gap.
    let data = [
        1, 2, 3, 4, 99, 99, 5, 6, 7, 8, 99, 99, 98, 98, 98, 98, 9, 10, 11, 12, 99, 99,
    ];
    let frame = Frame::new(
        &data,
        Layout {
            width: 4,
            height: 2,
            stride: 6,
            uv_offset: 16,
        },
    )
    .unwrap();
    // When packing for the original OpenCV input.
    let packed = frame.pack().unwrap();
    // Then only visible Y and UV bytes remain.
    assert_eq!(packed, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
}

#[test]
fn nv12_rejects_overlapping_uv_and_truncated_rows() {
    // Given a valid Y plane but an overlapping or truncated UV plane.
    for (uv_offset, length) in [(7, 12), (8, 11)] {
        let data = vec![0; length];
        let frame = Frame::new(
            &data,
            Layout {
                width: 4,
                height: 2,
                stride: 4,
                uv_offset,
            },
        )
        .unwrap();
        // When packing, then malformed planes cannot reach native image code.
        assert!(frame.pack().is_err());
    }
}

#[test]
fn gate_checks_both_configured_widths_and_rotates_without_speed_or_direction() {
    for (left_width, right_width, configured, expected) in [
        (2.0, 2.0, [true, true], vec![Side::Left, Side::Right]),
        (1.999, 4.0, [true, true], vec![Side::Right]),
        (4.0, 4.0, [false, true], vec![Side::Right]),
        (4.0, 4.0, [false, false], vec![]),
    ] {
        let result = gate(GateInput {
            alive_valid: true,
            left_width,
            right_width,
            configured,
            next_side: Side::Right,
        });
        assert_eq!(result.eligible_sides, expected);
        assert_eq!(result.side, expected.last().copied());
    }
}

#[test]
fn unavailable_messages_clear_both_widths_and_sides() {
    let result = gate(GateInput {
        alive_valid: false,
        left_width: 4.0,
        right_width: 4.0,
        configured: [true; 2],
        next_side: Side::Right,
    });
    assert!(!result.active);
    assert!(result.eligible_sides.is_empty());
    assert_eq!(result.lane_widths, [0.0; 2]);
    assert_eq!(result.side, None);
    assert_eq!(result.reason, "carState or modelV2 is unavailable");
}

#[test]
fn blindspot_hysteresis_retains_midrange_state_and_isolates_sides() {
    // Given active left detection after one full positive interval.
    let mut result = Blindspot::default();
    result.update(Side::Left, 0.9, 0.45, 0.2, 0.2);
    // When confidence falls for half the smoothing interval.
    result.update(Side::Left, 0.1, 0.45, 0.2, 0.1);
    // EMA retains the midpoint and preserves independent side state.
    assert!(result.side(Side::Left).active);
    assert_eq!(result.side(Side::Left).score, 0.5);
    assert!(!result.side(Side::Right).active);
}

#[test]
fn result_freshness_includes_deadline_and_rejects_future_or_zero() {
    // Given a timeout and monotonic result timestamps.
    let now = 2_000_000_000;
    // When testing freshness, then the inclusive boundary alone remains valid.
    assert!(fresh(1_000_000_000, now, 1_000_000_000));
    assert!(!fresh(999_999_999, now, 1_000_000_000));
    assert!(!fresh(now + 1, now, 1_000_000_000));
    assert!(!fresh(0, now, 1_000_000_000));
}

#[test]
fn threshold_and_strict_hysteresis_edges_match_final_classifier() {
    let mut result = Blindspot::default();
    result.update(Side::Left, 0.94, 0.94, 0.2, 0.2);
    assert!(result.side(Side::Left).active);
    result.update(Side::Left, 0.94 - 0.15, 0.94, 0.2, 0.2);
    assert!(result.side(Side::Left).active);
    result.update(Side::Left, 0.78, 0.94, 0.2, 0.2);
    assert!(!result.side(Side::Left).active);
    result.update(Side::Right, 0.0, 0.0, 0.2, 0.2);
    assert!(result.side(Side::Right).active);
}
