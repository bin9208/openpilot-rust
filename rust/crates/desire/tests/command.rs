#[test]
fn journal_timestamps_preserve_python_double_rounding() {
    for (encoded, expected) in [
        ("12.094999999999999", 12.094999999999999_f64),
        ("12.075000000000001", 12.075000000000001_f64),
    ] {
        let decoded: f64 = serde_json::from_str(encoded).unwrap();
        assert_eq!(decoded.to_bits(), expected.to_bits(), "{encoded}");
    }
}
