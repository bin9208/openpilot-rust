use openpilot_cluster::config::{normalize_cluster_panel_layout, normalize_cluster_screen_mode};

#[test]
fn cli_aliases_preserve_fullscreen_graph_and_panel_choices() {
    assert_eq!(normalize_cluster_screen_mode("\u{1c}Graph_RIGHT\u{85}"), 4);
    assert_eq!(normalize_cluster_screen_mode(" FULLSCREEN-3D "), -1);
    assert_eq!(normalize_cluster_screen_mode("-١"), -1);
    assert_eq!(normalize_cluster_screen_mode("7"), 0);
    assert_eq!(normalize_cluster_panel_layout(" CAMERA_RIGHT "), 1);
    assert_eq!(normalize_cluster_panel_layout("+００１"), 1);
    assert_eq!(normalize_cluster_panel_layout("1.0"), 0);
}
