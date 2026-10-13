use openpilot_jpeg::{Color, Layout};
#[cfg(feature = "native-skip-miri")]
use openpilot_jpeg::{Options, Quality};
#[test]
fn rgbx_contract_counts_four_components_and_alpha_does_not_change_jpeg() {
    let layout = Layout::new(2, 2, Color::Rgbx).unwrap();
    assert_eq!(layout.len(), 16);
    assert!(layout.check_pixels(&[0; 12]).is_err());
    #[cfg(feature = "native-skip-miri")]
    {
        let first = vec![
            12, 99, 201, 0, 23, 81, 172, 1, 92, 47, 25, 6, 123, 51, 8, 99,
        ];
        let mut second = first.clone();
        for p in second.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let options = Options::new(Quality::new(82).unwrap());
        assert_eq!(
            openpilot_jpeg::encode_with(&first, layout, options).unwrap(),
            openpilot_jpeg::encode_with(&second, layout, options).unwrap()
        );
        let rgb: Vec<_> = first
            .chunks_exact(4)
            .flat_map(|p| p[..3].iter().copied())
            .collect();
        assert_eq!(
            openpilot_jpeg::encode_with(&first, layout, options).unwrap(),
            openpilot_jpeg::encode_with(&rgb, Layout::new(2, 2, Color::Rgb).unwrap(), options)
                .unwrap()
        );
    }
}
