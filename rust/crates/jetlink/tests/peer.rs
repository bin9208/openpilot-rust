use openpilot_jetlink::peer::{classify, Kind};
#[test]
fn peer_allowlist_matches_1935_pinned_source_cases() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("peer_source_vectors.json")).unwrap();
    assert_eq!(
        fixture["source_commit"],
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let actual =
            classify(&case["peer"], case["path"].as_str().unwrap()).map(|kind| match kind {
                Kind::Mac => "usb",
                Kind::Android => "android",
                Kind::Ios => "ios",
            });
        assert_eq!(
            actual,
            case["expected"].as_str(),
            "source case {index}: {case}"
        );
    }
}
