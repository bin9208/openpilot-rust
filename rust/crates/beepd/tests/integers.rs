use openpilot_beepd::{integer, IntegerError};

#[test]
fn decimal_prefix_and_full_i32_range_follow_stoi() {
    for (input, expected) in [
        (b"".as_slice(), 0),
        (b" \t\n+6tail", 6),
        (b"6\0ignored", 6),
        (b"6.5", 6),
        (b"\x0b6", 6),
        (b"0x10", 0),
        (b"-2147483648tail", i32::MIN),
        (b"2147483647tail", i32::MAX),
    ] {
        assert_eq!(integer(input), Ok(expected));
    }
}

#[test]
fn missing_decimal_prefix_and_overflow_are_distinct_fatal_errors() {
    for input in [b" ".as_slice(), b"+", b"--1", b"\0", b"\xff", b"bad"] {
        assert_eq!(integer(input), Err(IntegerError::Invalid));
    }
    for input in [
        b"2147483648".as_slice(),
        b"-2147483649",
        b"999999999999999999999999999",
    ] {
        assert_eq!(integer(input), Err(IntegerError::Range));
    }
}
