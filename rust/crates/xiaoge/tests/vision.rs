use openpilot_xiaoge::nv12::{Frame, Layout};
use openpilot_xiaoge::vision::{fresh, gate, Blindspot, Direction, GateInput, Side};

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
fn gate_preserves_source_speed_and_target_lane_boundaries() {
    // Given live valid messages and a lane-change direction.
    for (speed, width, active) in [
        (30.0 / 3.6, 3.0, true),
        (120.0 / 3.6, 3.0, true),
        (29.9 / 3.6, 3.2, false),
        (120.1 / 3.6, 3.2, false),
        (20.0, 2.99, false),
    ] {
        let input = GateInput {
            alive_valid: true,
            speed,
            direction: Direction::Left,
            left_width: width,
            right_width: 7.0,
        };
        // When evaluating the gate, then only the intended side and boundaries enable inference.
        assert_eq!(gate(input).active, active);
    }
}

#[test]
fn unavailable_messages_clear_the_gate_side() {
    // Given a selected lane change with stale input.
    let input = GateInput {
        alive_valid: false,
        speed: 20.0,
        direction: Direction::Right,
        left_width: 4.0,
        right_width: 4.0,
    };
    // When evaluating, then neither a prior side nor activation survives.
    let result = gate(input);
    assert!(!result.active);
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
    // Then the left state stays active at score .5 and the untouched right stays false.
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
