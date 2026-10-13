use super::*;
use crate::models::{
    NaviDashboardState, NaviLiveState, NaviMediaFrame, NaviSpeedInfo, NaviTrafficLightInfo,
};

#[test]
fn empty_navigation_and_stalled_map_do_not_claim_visible_content() {
    assert!(!live_panel_visible(None));
    let mut navi = NaviLiveState {
        speed: Some(NaviSpeedInfo::default()),
        ..Default::default()
    };
    assert!(!live_panel_visible(Some(&navi)));
    navi.speed.as_mut().unwrap().secondary_sdi_type = Some(3);
    assert!(!live_panel_visible(Some(&navi)));
    navi.speed.as_mut().unwrap().sdi_type = Some(3);
    assert!(live_panel_visible(Some(&navi)));
    let mut dashboard = NaviDashboardState {
        connected: true,
        media: vec![NaviMediaFrame {
            key: "render:map_main".into(),
            present: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(map_frame_present(Some(&dashboard)));
    dashboard.map_stream_stalled = true;
    assert!(!map_frame_present(Some(&dashboard)));
    dashboard.map_stream_stalled = false;
    dashboard.connected = false;
    assert!(!map_frame_present(Some(&dashboard)));
}

#[test]
fn explicit_signal_flags_override_countdown_presence_and_priority() {
    let traffic = NaviTrafficLightInfo {
        red_s: Some(12),
        red_on: Some(false),
        left_s: Some(3),
        left_on: Some(true),
        straight_s: Some(5),
        straight_on: Some(true),
        ..Default::default()
    };
    assert!(!signal_active(Some(&traffic), Signal::Red, Some(12)));
    assert!(signal_active(Some(&traffic), Signal::Left, Some(0)));
    assert_eq!(
        primary_signal_seconds(Some(&traffic), [None; 5]),
        (Some(3), false)
    );
    assert_eq!(
        primary_signal_seconds(Some(&traffic), [None, Some(0), None, None, None]),
        (Some(0), false)
    );
    let fallback = NaviTrafficLightInfo {
        red_s: Some(0),
        red_on: Some(false),
        ..Default::default()
    };
    assert_eq!(
        primary_signal_seconds(Some(&fallback), [None; 5]),
        (Some(0), true)
    );
    assert_eq!(primary_signal_seconds(None, [Some(10); 5]), (None, false));
}
