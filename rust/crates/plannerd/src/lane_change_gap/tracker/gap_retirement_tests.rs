//! 원본 test_lane_change_gap.py SHA3bee725a/source841 성공에서 가져온 불변조건.
use super::{Input, Reason, Tracker};
use crate::lead::Lead;

#[test]
fn legacy_side_input_clears_nonempty_history_and_direction() {
    // 원본 legacy-side-lead 회귀의 secondary 유무 두 경우를 실제 tracker에 전달한다.
    // side_leads의 빈/비어 있지 않은 값은 같은 legacy_side_input=true wire로 표현된다.
    for include_secondary in [false, true] {
        let primary = Lead {
            status: true,
            radar: true,
            radar_track_id: 1,
            d_rel: 35.,
            v_lead: 15.,
            ..Lead::default()
        };
        let secondary = Lead {
            radar_track_id: 2,
            d_rel: 120.,
            y_rel: -3.6,
            ..primary
        };
        let mut input = Input {
            now: 1.,
            direction: 1,
            speed: 15.,
            yaw_rate: 0.04,
            path_t: Vec::new(),
            path_x: Vec::new(),
            path_y: Vec::new(),
            primary: Some(primary),
            secondary: Some(secondary),
            blindspot: false,
            valid: true,
            legacy_side_input: false,
        };
        let mut tracker = Tracker::default();
        tracker.update(&input).unwrap();
        assert_eq!(tracker.direction(), 1);
        assert!(
            !tracker.history.is_empty(),
            "real update must establish history"
        );

        input.now = 3.55;
        input.secondary = include_secondary.then_some(secondary);
        input.legacy_side_input = true;
        let plan = tracker.update(&input).unwrap();

        assert!(plan.active);
        assert_eq!(plan.reason, Reason::LegacySideInput);
        assert!(plan.targets.is_empty());
        assert_eq!(plan.confidence, 0.);
        assert_eq!(tracker.direction(), 0);
        assert!(
            tracker.history.is_empty(),
            "legacy input must revoke old evidence"
        );
    }
}
