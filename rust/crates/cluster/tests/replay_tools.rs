use openpilot_cluster::replay_tools::{
    fold_commands, format_time, option_rects, seek_value, Rect, ToolCommand,
};

#[test]
fn command_poll_preserves_toggle_parity_last_values_and_option_insertion_order() {
    let action = fold_commands([
        ToolCommand::TogglePause,
        ToolCommand::Seek { value: 5. },
        ToolCommand::TogglePause,
        ToolCommand::Seek { value: 15. },
        ToolCommand::Option {
            name: "road_camera".into(),
            value: false,
        },
        ToolCommand::Option {
            name: "route_loop".into(),
            value: true,
        },
        ToolCommand::Option {
            name: "road_camera".into(),
            value: true,
        },
        ToolCommand::OpenPath {
            value: " /owned/log/rlog.zst \n".into(),
        },
        ToolCommand::PreviousLog,
        ToolCommand::Closed,
    ])
    .expect("commands received");
    assert!(!action.toggle_pause);
    assert!(action.closed);
    assert!(action.previous_log);
    assert!(!action.next_log);
    assert_eq!(action.seek_s, Some(15.));
    assert_eq!(action.open_path.as_deref(), Some("/owned/log/rlog.zst"));
    assert_eq!(
        action.option_updates,
        vec![("road_camera".into(), true), ("route_loop".into(), true)]
    );
    assert!(fold_commands([]).is_none());
}
#[test]
fn tools_use_source_integer_layout_and_seek_clamping() {
    let boxes = option_rects(1100, 520).expect("valid layout");
    assert_eq!(boxes[0].1, Rect::new(24, 397, 342, 24));
    assert_eq!(boxes[4].1, Rect::new(374, 422, 342, 24));
    assert_eq!(
        seek_value(-100, Rect::new(24, 478, 1052, 16), 300.).to_bits(),
        0_f64.to_bits()
    );
    assert_eq!(
        seek_value(550, Rect::new(24, 478, 1052, 16), 300.).to_bits(),
        150_f64.to_bits()
    );
    assert_eq!(
        seek_value(3000, Rect::new(24, 478, 1052, 16), 300.).to_bits(),
        300_f64.to_bits()
    );
}
#[test]
fn time_labels_truncate_before_nonnegative_clamping() {
    assert_eq!(format_time(-0.1).expect("finite"), "0:00");
    assert_eq!(format_time(125.99).expect("finite"), "2:05");
    assert!(format_time(f64::NAN).is_err());
}

#[test]
fn latest_state_replaces_unconsumed_updates_and_throttle_keeps_source_boundary() {
    let queue = openpilot_cluster::replay_tools::queue::Latest::default();
    let mut throttle = openpilot_cluster::replay_tools::queue::UpdateThrottle::default();
    queue.replace(1);
    queue.replace(2);
    queue.replace(3);
    assert_eq!(queue.take(), Some(3));
    assert_eq!(queue.take(), None);
    assert!(!throttle.accepts(0.099));
    assert!(throttle.accepts(0.1));
    assert!(!throttle.accepts(0.199));
    assert!(throttle.accepts(0.2));
}
#[test]
fn saved_folder_uses_existing_directory_and_invalid_utf8_is_not_silently_swallowed() {
    let root = tempfile::tempdir().expect("owned directory");
    let file = root.path().join("last");
    std::fs::write(&file, format!(" \t{}\n", root.path().display())).expect("write");
    assert_eq!(
        openpilot_cluster::replay_tools::load_last_folder(&file).expect("load"),
        root.path().display().to_string()
    );
    std::fs::write(&file, [0xff]).expect("write invalid UTF-8");
    assert!(openpilot_cluster::replay_tools::load_last_folder(&file).is_err());
}
