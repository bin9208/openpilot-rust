use openpilot_cluster::models::ClusterUiState;

#[test]
fn complete_shared_state_default_matches_original_standby() {
    let original: serde_json::Value =
        serde_json::from_str(include_str!("data/models_standby.json")).unwrap();
    let actual = serde_json::to_value(ClusterUiState::default()).unwrap();
    assert_eq!(actual.as_object().unwrap().len(), 149);
    assert_eq!(actual["external_compute_label"], "");
    assert_eq!(actual, original);
}
