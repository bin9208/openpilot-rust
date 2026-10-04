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

#[test]
fn typed_rgb75_remains_identical_and_new_outputs_own_their_bytes() {
    use openpilot_jpeg::{encode, encode_with, Color, Layout, Options, Quality};
    let rgb = vec![128; 8 * 4 * 3];
    let rgb_layout = Layout::new(8, 4, Color::Rgb).unwrap();
    let quality75 = Options::new(Quality::new(75).unwrap());
    assert_eq!(
        encode(&rgb, 8, 4).unwrap(),
        encode_with(&rgb, rgb_layout, quality75).unwrap()
    );
    let gray_layout = Layout::new(8, 4, Color::Gray).unwrap();
    let gray = {
        let data = vec![127; gray_layout.len()];
        encode_with(&data, gray_layout, Options::new(Quality::new(50).unwrap())).unwrap()
    };
    let again = encode_with(
        &vec![127; gray_layout.len()],
        gray_layout,
        Options::new(Quality::new(50).unwrap()),
    )
    .unwrap();
    assert_eq!(gray, again);
    assert!(encode_with(&[0; 2], gray_layout, quality75).is_err());
}
