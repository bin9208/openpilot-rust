use openpilot_bluetooth::{decode_events, InputError};

#[test]
fn signed_linux_events_preserve_layout_and_binary64_timestamps() {
    for (seconds, micros, kind, code, value, timestamp_bits) in [
        (
            123_i64,
            999999_i64,
            1_u16,
            115_u16,
            -1_i32,
            0x405e_ffff_fbce_4218,
        ),
        (
            i64::MIN,
            i64::MAX,
            u16::MAX,
            u16::MAX,
            i32::MIN,
            0xc3df_fffd_e721_0be9,
        ),
        (0, -1, 0, 3, i32::MAX, 0xbeb0_c6f7_a0b5_ed8d),
    ] {
        let bytes = [
            seconds.to_ne_bytes().as_slice(),
            micros.to_ne_bytes().as_slice(),
            kind.to_ne_bytes().as_slice(),
            code.to_ne_bytes().as_slice(),
            value.to_ne_bytes().as_slice(),
        ]
        .concat();
        let events = decode_events(&bytes).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].at.0.to_bits(), timestamp_bits);
        assert_eq!(
            (events[0].kind, events[0].code, events[0].value),
            (kind, code, value)
        );
        for length in 0..24 {
            assert!(matches!(
                decode_events(&bytes[..length]),
                Err(InputError::Disconnected)
            ));
        }
    }
}
