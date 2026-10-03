use openpilot_camerad::geometry::{luminance, Region, Sampling};

#[test]
fn upper_histogram_median_preserves_the_source_small_sample_rule() {
    let one = Region {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    assert_eq!(
        luminance(
            &[17],
            one,
            Sampling {
                width: 1,
                x_skip: 1,
                y_skip: 1
            }
        )
        .unwrap()
        .to_bits(),
        (255.0_f32 / 256.0).to_bits()
    );
    let four = Region {
        x: 0,
        y: 0,
        width: 4,
        height: 1,
    };
    assert_eq!(
        luminance(
            &[0, 1, 200, 255],
            four,
            Sampling {
                width: 4,
                x_skip: 1,
                y_skip: 1
            }
        )
        .unwrap()
        .to_bits(),
        (200.0_f32 / 256.0).to_bits()
    );
}
