use openpilot_registration::utf7::{decode_replace, json_text};
#[test]
fn malformed_shift_is_replaced_and_surrogate_is_kept_as_a_codepoint() {
    assert_eq!(decode_replace(b"+A-"), vec![0xfffd]);
    assert_eq!(decode_replace(b"+2AA-"), vec![0xd800]);
}
#[test]
fn surrogate_json_escape_preserves_post_decode_failure_boundary() {
    let points: Vec<u32> = "{\"dongle_id\":\""
        .chars()
        .map(u32::from)
        .chain([0xd800])
        .chain("\"}".chars().map(u32::from))
        .collect();
    assert_eq!(json_text(&points).unwrap(), "{\"dongle_id\":\"\\ud800\"}");
    assert!(json_text(&[u32::from('\\'), 0xd800]).is_err());
}
