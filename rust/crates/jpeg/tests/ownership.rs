#![cfg(feature = "native-skip-miri")]
#[test]
fn owned_output_and_checked_rgb_dimensions() {
    let rgb = vec![128; 8 * 4 * 3];
    let first = openpilot_jpeg::encode(&rgb, 8, 4).unwrap();
    assert_eq!(&first[..2], &[255, 216]);
    assert_eq!(&first[first.len() - 2..], &[255, 217]);
    assert_eq!(first, openpilot_jpeg::encode(&rgb, 8, 4).unwrap());
    assert!(openpilot_jpeg::encode(&rgb[..rgb.len() - 1], 8, 4).is_err());
    assert!(openpilot_jpeg::encode(&[], 0, 0).is_err());
    assert!(openpilot_jpeg::encode(&[], u32::MAX, u32::MAX).is_err());
}
