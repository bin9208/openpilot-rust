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

#[test]
fn compact_samples_preserve_original_offsets_median_and_validation() {
    use openpilot_camerad::geometry::{luminance_samples, sample_indices};
    let pixels: Vec<_> = (0..4096).map(|index| (index * 31 % 256) as u8).collect();
    for width in [0, 1, 7, 8, 13, 64] {
        for region_width in [0, 1, 2, 5, 9] {
            for height in [0, 1, 2, 5, 11] {
                for x_skip in [0, 1, 2, 3, usize::MAX] {
                    for y_skip in [0, 1, 2, 4, usize::MAX] {
                        let region = Region {
                            x: 0,
                            y: 1,
                            width: region_width,
                            height,
                        };
                        let sampling = Sampling {
                            width,
                            x_skip,
                            y_skip,
                        };
                        for length in [0, 1, 7, 100, pixels.len()] {
                            let old = luminance(&pixels[..length], region, sampling);
                            let plan = sample_indices(region, sampling, length);
                            assert_eq!(
                                old.is_ok(),
                                plan.is_ok(),
                                "{region:?} {sampling:?} len={length}"
                            );
                            if let (Ok(old), Ok(plan)) = (old, plan) {
                                let expected: Vec<_> = if x_skip != 0 && y_skip != 0 {
                                    (1..1 + height as usize)
                                        .step_by(y_skip)
                                        .flat_map(|row| {
                                            (0..region_width as usize)
                                                .step_by(x_skip)
                                                .map(move |x| row * width + x)
                                        })
                                        .collect()
                                } else {
                                    unreachable!()
                                };
                                assert_eq!(plan.clone().collect::<Vec<_>>(), expected);
                                assert_eq!(plan.len(), expected.len());
                                let samples: Vec<_> = plan.map(|offset| pixels[offset]).collect();
                                assert_eq!(old.to_bits(), luminance_samples(&samples).to_bits());
                            }
                        }
                    }
                }
            }
        }
    }
    for region in [
        Region {
            x: -1,
            y: 0,
            width: 1,
            height: 1,
        },
        Region {
            x: 0,
            y: -1,
            width: 1,
            height: 1,
        },
        Region {
            x: 0,
            y: 0,
            width: -1,
            height: 1,
        },
        Region {
            x: 0,
            y: 0,
            width: 1,
            height: -1,
        },
    ] {
        assert!(sample_indices(
            region,
            Sampling {
                width: 8,
                x_skip: 2,
                y_skip: 2
            },
            4096
        )
        .is_err());
    }
    assert_eq!(
        luminance_samples(&[]).to_bits(),
        (255_f32 / 256.0).to_bits()
    );
    assert_eq!(
        luminance_samples(&[17]).to_bits(),
        (255_f32 / 256.0).to_bits()
    );
}
