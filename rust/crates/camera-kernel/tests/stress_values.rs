#![cfg(feature = "native-skip-miri")]
use openpilot_camera_kernel::{parse_double_prefix, DoubleParseError};

#[test]
fn double_parser_preserves_cpp_prefix_and_range_failures() {
    assert_eq!(parse_double_prefix(c"\t +0.25 trailing").unwrap(), 0.25);
    assert_eq!(parse_double_prefix(c"0x1.8p+1rest").unwrap(), 3.0);
    assert!(parse_double_prefix(c"nan").unwrap().is_nan());
    assert_eq!(
        parse_double_prefix(c"-infinity").unwrap(),
        f64::NEG_INFINITY
    );
    assert!(matches!(
        parse_double_prefix(c""),
        Err(DoubleParseError::InvalidArgument)
    ));
    assert!(matches!(
        parse_double_prefix(c"x"),
        Err(DoubleParseError::InvalidArgument)
    ));
    assert!(matches!(
        parse_double_prefix(c"1e9999"),
        Err(DoubleParseError::OutOfRange)
    ));
    assert!(matches!(
        parse_double_prefix(c"1e-9999"),
        Err(DoubleParseError::OutOfRange)
    ));
}

#[test]
fn cpp_double_parser_restores_preexisting_errno_when_conversion_does_not_set_it() {
    for value in [c"1.25suffix", c"invalid"] {
        let failure = std::fs::File::open("/proc/0/camera-parser-missing").unwrap_err();
        let previous = failure.raw_os_error().unwrap();
        let _result = parse_double_prefix(value);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(previous)
        );
    }
}
