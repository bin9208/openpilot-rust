use openpilot_card::brands::hyundai::lead::{self, Lead};
#[test]
fn pinned_scc_hud_receding_threshold_preserves_white_near_standstill() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/scc-hud-final.json")).unwrap();
    for row in rows {
        let lead = Lead {
            status: true,
            radar: true,
            radar_track_id: 3,
            d_rel: 30.,
            y_rel: -1.,
            v_rel: row["velocity"].as_f64().unwrap(),
        };
        let actual = lead::scc_fields(&[lead], (&[], &[]), None).unwrap();
        assert_eq!(
            actual,
            serde_json::from_value::<[f64; 4]>(row["fields"].clone()).unwrap(),
            "velocity {}",
            lead.v_rel
        );
    }
}
