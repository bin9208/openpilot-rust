pub(super) const JSON: &[&str] = &[
    "vehicle",
    "guidance_current",
    "guidance_next",
    "lane_current",
    "lane_ahead",
    "speed",
    "traffic_signal",
    "crossroad",
    "route",
    "navigation_status",
    "app_status",
    "camera_state",
    "composition_state",
];
pub(super) const IMAGE: &[&str] = &[
    "tbt_current_compact",
    "tbt_current_full",
    "tbt_next",
    "traffic_signal",
    "lane_top",
    "lane_bottom",
    "safety_primary",
    "safety_secondary",
    "safety_section",
    "crossroad_minimized",
    "crossroad_expanded",
    "center_tbt_icon",
    "center_tbt_text",
    "center_tbt_fee",
];
pub(super) fn waiting() -> Vec<crate::models::NaviItemStatus> {
    [
        ("json", JSON),
        ("image", IMAGE),
        ("render", ["map_main"].as_slice()),
    ]
    .into_iter()
    .flat_map(|(kind, names)| {
        names.iter().map(move |name| crate::models::NaviItemStatus {
            key: format!("{kind}:{name}"),
            reason: Some("waiting".into()),
            ..crate::models::NaviItemStatus::default()
        })
    })
    .collect()
}
