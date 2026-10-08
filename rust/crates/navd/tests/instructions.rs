use openpilot_navd::instructions::{parse_banner_instructions, string_to_direction, Direction};
use serde_json::json;

#[test]
fn direction_substring_priority_matches_original() {
    assert_eq!(
        string_to_direction("slight left right"),
        Direction::SlightLeft
    );
    assert_eq!(string_to_direction("right straight"), Direction::Right);
    assert_eq!(string_to_direction("slight straight"), Direction::Straight);
    assert_eq!(string_to_direction("uturn"), Direction::None);
}

#[test]
fn banner_uses_last_matching_threshold_and_preserves_optional_lanes() {
    let banners = json!([
        {"distanceAlongGeometry":500,"primary":{"text":"far","type":"turn","modifier":"left"}},
        {"distanceAlongGeometry":100,"primary":{"text":"near","type":null},
         "secondary":{"text":"Exit 2"},
         "sub":{"components":[{"type":"text"},{"type":"lane","active":true,
         "directions":["slight left","straight"],"active_direction":"slight left"}]}}
    ]);
    let far = parse_banner_instructions(&banners, 100.).unwrap().unwrap();
    assert_eq!(far.maneuver_primary_text.as_deref(), Some("far"));
    let near = parse_banner_instructions(&banners, 99.).unwrap().unwrap();
    assert_eq!(near.maneuver_primary_text.as_deref(), Some("near"));
    assert!(near.maneuver_type.is_none());
    assert_eq!(near.maneuver_secondary_text.as_deref(), Some("Exit 2"));
    assert!(near.show_full);
    let lanes = near.lanes.unwrap();
    assert_eq!(lanes.len(), 1);
    assert!(lanes[0].active);
    assert_eq!(
        lanes[0].directions,
        vec![Direction::SlightLeft, Direction::Straight]
    );
    assert_eq!(lanes[0].active_direction, Some(Direction::SlightLeft));
}

#[test]
fn exact_threshold_keeps_first_banner_but_hides_full_instruction() {
    let banners = json!([{"distanceAlongGeometry":500,"primary":{"text":"turn"}}]);
    let instruction = parse_banner_instructions(&banners, 500.).unwrap().unwrap();
    assert!(!instruction.show_full);
    assert_eq!(instruction.maneuver_primary_text.as_deref(), Some("turn"));
    assert!(parse_banner_instructions(&json!([]), 0.).unwrap().is_none());
}
