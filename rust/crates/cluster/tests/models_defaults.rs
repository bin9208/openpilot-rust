use openpilot_cluster::models::*;
fn record<T: Default + serde::Serialize>(name: &str) -> (String, serde_json::Value) {
    (name.to_owned(), serde_json::to_value(T::default()).unwrap())
}
#[test]
fn all_source_record_defaults_match() {
    let actual: serde_json::Map<String, serde_json::Value> = [
        record::<GitBranchStatus>("GitBranchStatus"),
        record::<ModelPathPoint>("ModelPathPoint"),
        record::<ModelRiskPoint>("ModelRiskPoint"),
        record::<LaneMarking>("LaneMarking"),
        record::<RouteOverlay>("RouteOverlay"),
        record::<LiveDebugInfo>("LiveDebugInfo"),
        record::<DebugPlotSnapshot>("DebugPlotSnapshot"),
        record::<TripReportState>("TripReportState"),
        record::<NaviItemMeta>("NaviItemMeta"),
        record::<NaviVehicleInfo>("NaviVehicleInfo"),
        record::<NaviGuidanceInfo>("NaviGuidanceInfo"),
        record::<NaviLaneInfo>("NaviLaneInfo"),
        record::<NaviSpeedInfo>("NaviSpeedInfo"),
        record::<NaviCrossroadInfo>("NaviCrossroadInfo"),
        record::<NaviRouteInfo>("NaviRouteInfo"),
        record::<NaviStatusInfo>("NaviStatusInfo"),
        record::<NaviLiveState>("NaviLiveState"),
        record::<NaviDebugInfo>("NaviDebugInfo"),
        record::<NaviTrafficLightInfo>("NaviTrafficLightInfo"),
        record::<NaviGuidanceImage>("NaviGuidanceImage"),
        record::<NaviMediaFrame>("NaviMediaFrame"),
        record::<NaviItemStatus>("NaviItemStatus"),
        record::<NaviDashboardState>("NaviDashboardState"),
        record::<DetectedVehicle>("DetectedVehicle"),
        record::<RadarPoint>("RadarPoint"),
        record::<SimulatorInput>("SimulatorInput"),
        record::<TpmsInfo>("TpmsInfo"),
        record::<ClusterAlert>("ClusterAlert"),
        record::<ClusterUiState>("ClusterUiState"),
        record::<SceneCamera>("SceneCamera"),
    ]
    .into_iter()
    .collect();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("data/models_defaults.json")).unwrap();
    assert_eq!(actual["ClusterUiState"]["external_compute_label"], "");
    assert_eq!(serde_json::Value::Object(actual), expected);
}
