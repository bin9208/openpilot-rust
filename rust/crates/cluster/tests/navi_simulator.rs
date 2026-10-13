use openpilot_carrot_navi::{json::Value, manifest::MapConfig, receiver::Receiver};
use openpilot_cluster::navi_simulator::Projection;

#[test]
fn disconnected_receiver_retains_simulated_world_and_waiting_dashboard() {
    let receiver = Receiver::new(Value::integer(7714), MapConfig::default());
    let mut projection = Projection::new("127.0.0.1:7714".into(), 10.0);
    assert_eq!(
        projection.status_text().unwrap(),
        "navi 127.0.0.1:7714 starting"
    );
    let state = projection
        .update_at(
            &receiver.dashboard_snapshot(),
            &receiver.cereal_snapshot(),
            10.05,
            100_000,
            "12:34:56",
        )
        .unwrap();
    assert!(!state.onroad);
    assert_eq!(state.speed_kph.to_bits(), 0.0_f64.to_bits());
    assert_eq!(state.center_clock_text.as_deref(), Some("12:34:56"));
    assert!(state.navi_live.is_none());
    let dashboard = state.navi_dashboard.unwrap();
    assert_eq!(dashboard.items.len(), 28);
    assert!(dashboard
        .items
        .iter()
        .all(|item| !item.present && item.reason.as_deref() == Some("waiting")));
    assert_eq!(dashboard.app_status, "APP waiting");
    assert_eq!(
        projection.status_text().unwrap(),
        "navi connected=0 app=- items=0/28 received=0 map=0/-"
    );
    projection.close();
}
