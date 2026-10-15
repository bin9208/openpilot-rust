#[path = "support/trace.rs"]
mod trace;
#[test]
fn original_final_protocol_config_frames_acknowledgements_and_handshake() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/protocol.json")).unwrap();
    assert_eq!(
        fixture["source_commit"],
        "e8e152dbcfd9574a1791ed12026d1880e87689b3"
    );
    let cases = fixture["input"].as_array().unwrap();
    let expected = fixture["expected"].as_array().unwrap();
    assert_eq!(cases.len(), 149);
    for (index, (case, expected)) in cases.iter().zip(expected).enumerate() {
        assert_eq!(trace::run(case), *expected, "source case {index}: {case}");
    }
}
