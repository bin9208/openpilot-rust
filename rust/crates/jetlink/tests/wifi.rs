use openpilot_jetlink::{wifi, Error};
use serde_json::json;
#[test]
fn private_profile_validation_matches_original_source_cases() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("wifi_source_vectors.json")).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            wifi::validate(&case["value"]).is_ok(),
            case["valid"].as_bool().unwrap(),
            "source case {index}"
        );
    }
}
#[test]
fn collector_keeps_active_first_skips_access_points_and_fails_missing_secrets() {
    let active = "a".repeat(36);
    let saved = "b".repeat(36);
    let access_point = "c".repeat(36);
    let collect = |missing: bool| {
        wifi::collect_with(|args| {
            Ok(if args.contains(&"--active") {
                active.clone()
            } else if args.contains(&"UUID,TYPE") {
                format!("{saved}:802-11-wireless\n{active}:802-11-wireless\n{access_point}:802-11-wireless")
            } else if args.last() == Some(&access_point.as_str()) {
                "fixture-ap\nap\nfalse\n\n".into()
            } else if missing {
                "fixture\ninfrastructure\nno\nwpa-psk\n".into()
            } else {
                "fixture\ninfrastructure\nyes\nwpa-psk\ntest-only-secret".into()
            })
        })
    };
    let profiles = collect(false).unwrap();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0]["id"], active);
    assert_eq!(profiles[0]["active"], true);
    assert_eq!(profiles[1]["active"], false);
    assert!(collect(true).is_err());
    assert!(wifi::collect_with(|_| Err(Error::Contract("query unavailable"))).is_err());
    assert!(wifi::validate(&json!({"version":1,"owner":"a".repeat(64),"profiles":[]})).is_ok());
}
