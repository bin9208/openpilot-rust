#![cfg(target_os = "linux")]

use openpilot_dev_tools::hardware_info_qa::{
    canonical_float_hex, decode_hex, encode_hex, valid_case_name,
};

#[test]
fn canonical_float_hex_matches_python_hex_spelling() {
    assert_eq!(canonical_float_hex(0.0), "0x0.0p+0");
    assert_eq!(canonical_float_hex(-0.0), "-0x0.0p+0");
    assert_eq!(canonical_float_hex(1.0), "0x1.0000000000000p+0");
    assert_eq!(canonical_float_hex(1.5), "0x1.8000000000000p+0");
    assert_eq!(
        canonical_float_hex(f64::from_bits(1)),
        "0x0.0000000000001p-1022"
    );
    assert_eq!(canonical_float_hex(f64::INFINITY), "inf");
    assert_eq!(canonical_float_hex(f64::NEG_INFINITY), "-inf");
    assert_eq!(canonical_float_hex(f64::NAN), "nan");
}

#[test]
fn packet_hex_is_strict_and_round_trips() {
    let bytes = [0, 1, 15, 16, 127, 128, 255];
    assert_eq!(encode_hex(&bytes), "00010f107f80ff");
    assert_eq!(decode_hex("00010f107f80ff").unwrap(), bytes);
    assert!(decode_hex("0").is_err());
    assert!(decode_hex("0g").is_err());
    assert!(decode_hex("é").is_err());
}

#[test]
fn plan_names_cannot_escape_case_output_directory() {
    for name in ["", "../case", "nested/name", "space name", "é"] {
        assert!(!valid_case_name(name));
    }
    for name in ["case", "case-17", "case_17"] {
        assert!(valid_case_name(name));
    }
}
