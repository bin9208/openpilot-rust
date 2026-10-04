use openpilot_jpeg::{Color, ContractError, Layout, Options, Quality};

#[test]
fn jpeg_layout_checks_component_count_extent_and_borrowed_length() {
    assert_eq!(
        Layout::new(0, 4, Color::Rgb),
        Err(ContractError::Dimensions)
    );
    assert_eq!(
        Layout::new(4, 65501, Color::Gray),
        Err(ContractError::Dimensions)
    );
    let rgb = Layout::new(4, 2, Color::Rgb).unwrap();
    let gray = Layout::new(4, 2, Color::Gray).unwrap();
    assert_eq!(rgb.len(), 24);
    assert_eq!(gray.len(), 8);
    assert!(rgb.check_pixels(&[0; 23]).is_err());
    assert!(gray.check_pixels(&[0; 9]).is_err());
    assert!(gray.check_pixels(&[0; 8]).is_ok());
}

#[test]
fn quality_is_validated_before_native_codec_setup() {
    assert_eq!(Quality::new(0), Err(ContractError::Quality));
    assert_eq!(Quality::new(101), Err(ContractError::Quality));
    for value in [1, 50, 75, 85, 100] {
        assert_eq!(
            Options::new(Quality::new(value).unwrap()).quality().value(),
            value
        );
    }
}
