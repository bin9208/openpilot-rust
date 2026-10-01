use openpilot_qcomgpsd::nmea::{checksum_delimiter, parse};
#[test]
fn source_delimiter_does_not_validate_checksum_bytes() {
    assert!(checksum_delimiter("$GNCLK,1,18,0,0,-100,0,0,0,0,,*ZZ"));
    assert!(!checksum_delimiter("$GNCLK,1*1"));
    assert!(!checksum_delimiter("$GNCLK,1*123"));
    assert!(!checksum_delimiter("$GNCLK,1"));
}
#[test]
fn empty_fields_and_string_zero_have_distinct_values() {
    let message = parse("$GNCLK,1,,0,0,-100,0,0,0,0,,*ZZ").unwrap().unwrap();
    assert_eq!(message.to_string(), "GnssClockNmeaPort(flags=1, leap_seconds=None, time_ns=0, time_uncertainty_ns=0, full_bias_ns=-100, bias_ns=0.0, bias_uncertainty_ns=0.0, drift_nsps=0.0, drift_uncertainty_nsps=0.0)");
    assert!(parse("$GNMEAS,1*00").is_err());
}
