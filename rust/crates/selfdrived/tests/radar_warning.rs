use openpilot_selfdrived::radar_only_warning::{Point, Warning};
#[test]
fn pinned_source_front_corner_confirmation_rejection_cooldown_and_rearm_trace() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/radar-warning-final.json")).unwrap();
    let mut warning = Warning::default();
    let mut alerts = 0;
    for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
        if row["reset"] == true {
            warning = Warning::default();
        }
        let model: openpilot_radard::model::Model =
            serde_json::from_value(row["model"].clone()).unwrap();
        let lane = openpilot_radard::stationary_lane::confident_ego_lane(&model);
        let points: Vec<Point> = serde_json::from_value(row["points"].clone()).unwrap();
        let probs: Vec<f64> = serde_json::from_value(row["vision_probs"].clone()).unwrap();
        let result = warning.update(
            row["now"].as_f64().unwrap(),
            row["v_ego"].as_f64().unwrap(),
            lane.as_ref().map(|(l, _)| l),
            &probs,
            &points,
            true,
            row["selected"].as_bool().unwrap(),
        );
        assert_eq!(result, row["expected"].as_bool().unwrap(), "source row {i}");
        alerts += usize::from(result);
    }
    assert!(alerts >= 2);
}
