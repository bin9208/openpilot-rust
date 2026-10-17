#![cfg(feature = "native-skip-miri")]
#[test]
fn decoded_gray_and_rgb_match_our_encoder_layout() {
    for (components, color) in [
        (1, openpilot_jpeg::Color::Gray),
        (3, openpilot_jpeg::Color::Rgb),
    ] {
        let input = vec![90; 8 * 8 * components];
        let layout = openpilot_jpeg::Layout::new(8, 8, color).unwrap();
        let encoded = openpilot_jpeg::encode_with(
            &input,
            layout,
            openpilot_jpeg::Options::new(openpilot_jpeg::Quality::new(75).unwrap()),
        )
        .unwrap();
        let decoded = openpilot_jpeg::decode(&encoded, 64).unwrap();
        assert_eq!(
            (decoded.width, decoded.height, decoded.components),
            (8, 8, components as u32)
        );
        assert_eq!(decoded.pixels, input);
        assert!(openpilot_jpeg::decode(&encoded, 63).is_err());
        assert!(openpilot_jpeg::decode(&encoded[..encoded.len() - 1], 64).is_err());
    }
}

#[test]
fn repeated_invalid_streams_recover_without_returning_partial_pixels() {
    for _ in 0..64 {
        for input in [
            &[][..],
            &[255, 216, 255, 217][..],
            &[255, 216, 255][..],
            b"not jpeg",
        ] {
            assert!(openpilot_jpeg::decode(input, 178_956_970).is_err());
        }
    }
}
