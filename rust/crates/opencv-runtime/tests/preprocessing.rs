#![cfg(feature = "native-skip-miri")]

use openpilot_opencv_runtime::{self as cv, Dimensions, Format, ImageLayout, ImageView, Rect};

#[test]
fn composite_bsd_pixels_match_separate_conversion_mask_and_resize() {
    cv::initialize(2).unwrap();
    for (width, height) in [(2, 2), (10, 6), (6, 14)] {
        let size = Dimensions::new(width, height).unwrap();
        let nv12: Vec<_> = (0..size.nv12_len().unwrap())
            .map(|i| (i.wrapping_mul(73) % 256) as u8)
            .collect();
        for pattern in 0..3 {
            let mask: Vec<_> = (0..size.pixels().unwrap())
                .map(|i| match pattern {
                    0 => 0,
                    1 => 255,
                    _ => {
                        if i % 3 == 0 {
                            7
                        } else {
                            0
                        }
                    }
                })
                .collect();
            let view =
                ImageView::new(&mask, ImageLayout::new(size, Format::Gray).unwrap()).unwrap();
            for output in [
                size,
                Dimensions::new(17, 11).unwrap(),
                Dimensions::new(3, 2).unwrap(),
            ] {
                let expected = cv::resize_linear(
                    cv::apply_mask(cv::nv12_to_rgb(&nv12, size).unwrap().view(), view)
                        .unwrap()
                        .view(),
                    output,
                )
                .unwrap();
                let before = nv12.clone();
                let actual = cv::nv12_masked_resize_rgb(&nv12, size, view, output).unwrap();
                assert_eq!(
                    actual.view().data(),
                    expected.view().data(),
                    "{width}x{height}, pattern {pattern}"
                );
                assert_eq!(actual.view().layout(), expected.view().layout());
                assert_eq!(nv12, before);
            }
        }
    }
    let owned = {
        let nv12 = [90; 12];
        let mask = [0; 8];
        let size = Dimensions::new(4, 2).unwrap();
        cv::nv12_masked_resize_rgb(
            &nv12,
            size,
            ImageView::new(&mask, ImageLayout::new(size, Format::Gray).unwrap()).unwrap(),
            size,
        )
        .unwrap()
    };
    assert_eq!(owned.view().data(), &[0; 24]);
}

#[test]
fn composite_rejects_malformed_nv12_and_mask_layouts() {
    cv::initialize(2).unwrap();
    let size = Dimensions::new(4, 2).unwrap();
    let mask = ImageView::new(&[255; 8], ImageLayout::new(size, Format::Gray).unwrap()).unwrap();
    for input in [&[0; 11][..], &[0; 13][..]] {
        assert!(cv::nv12_masked_resize_rgb(input, size, mask, size).is_err());
    }
    let odd = Dimensions::new(3, 2).unwrap();
    assert!(cv::nv12_masked_resize_rgb(&[0; 9], odd, mask, size).is_err());
    let rgb_mask =
        ImageView::new(&[255; 24], ImageLayout::new(size, Format::Rgb).unwrap()).unwrap();
    assert!(cv::nv12_masked_resize_rgb(&[0; 12], size, rgb_mask, size).is_err());
    let small = Dimensions::new(2, 2).unwrap();
    let small_mask =
        ImageView::new(&[255; 4], ImageLayout::new(small, Format::Gray).unwrap()).unwrap();
    assert!(cv::nv12_masked_resize_rgb(&[0; 12], size, small_mask, size).is_err());
}

#[test]
fn preprocessing_outputs_survive_input_mutation_drop_and_subsequent_calls() {
    cv::initialize(2).unwrap();
    let size = Dimensions::new(4, 4).unwrap();
    let output = Dimensions::new(7, 5).unwrap();
    let mut raw: Vec<_> = (0..24).map(|i| (i * 19) as u8).collect();
    let mut mask = vec![255; 16];
    let filter = ImageView::new(&mask, ImageLayout::new(size, Format::Gray).unwrap()).unwrap();
    let expected = cv::resize_linear(
        cv::apply_mask(cv::nv12_to_rgb(&raw, size).unwrap().view(), filter)
            .unwrap()
            .view(),
        output,
    )
    .unwrap()
    .into_data();
    let actual = cv::nv12_masked_resize_rgb(&raw, size, filter, output).unwrap();
    raw.fill(0);
    mask.fill(0);
    let next = cv::nv12_masked_resize_rgb(
        &raw,
        size,
        ImageView::new(&mask, ImageLayout::new(size, Format::Gray).unwrap()).unwrap(),
        output,
    )
    .unwrap();
    assert_eq!(next.view().data(), &[0; 105]);
    drop(raw);
    drop(mask);
    assert_eq!(actual.view().data(), expected);

    let size = Dimensions::new(5, 5).unwrap();
    let roi = Rect {
        x: 1,
        y: 1,
        width: 3,
        height: 4,
    };
    let output = Dimensions::new(7, 6).unwrap();
    let mut raw: Vec<_> = (0..35).map(|i| (i * 7) as u8).collect();
    let packed = [56, 63, 70, 105, 112, 119, 154, 161, 168, 203, 210, 217];
    let expected = cv::resize_linear(
        ImageView::new(
            &packed,
            ImageLayout::new(Dimensions::new(3, 4).unwrap(), Format::Gray).unwrap(),
        )
        .unwrap(),
        output,
    )
    .unwrap()
    .into_data();
    let actual = cv::resize_gray_roi(&raw, size, 7, roi, output).unwrap();
    raw.fill(0);
    let next = cv::resize_gray_roi(&raw, size, 7, roi, output).unwrap();
    assert_eq!(next.view().data(), &[0; 42]);
    drop(raw);
    assert_eq!(actual.view().data(), expected);
}

#[test]
fn strided_gray_roi_matches_packed_resize_with_odd_origins_and_strides() {
    cv::initialize(2).unwrap();
    let size = Dimensions::new(9, 7).unwrap();
    for stride in [9, 11, 16] {
        let bytes: Vec<_> = (0..stride * 7).map(|i| (i * 31 % 256) as u8).collect();
        let unchanged = bytes.clone();
        for roi in [
            Rect {
                x: 1,
                y: 2,
                width: 5,
                height: 3,
            },
            Rect {
                x: 0,
                y: 0,
                width: 9,
                height: 7,
            },
            Rect {
                x: 8,
                y: 6,
                width: 1,
                height: 1,
            },
        ] {
            let crop_size = Dimensions::new(roi.width, roi.height).unwrap();
            let mut packed = Vec::new();
            for y in roi.y as usize..roi.y as usize + roi.height as usize {
                let start = y * stride + roi.x as usize;
                packed.extend_from_slice(&bytes[start..start + roi.width as usize]);
            }
            let view = ImageView::new(&packed, ImageLayout::new(crop_size, Format::Gray).unwrap())
                .unwrap();
            for output in [
                crop_size,
                Dimensions::new(416, 416).unwrap(),
                Dimensions::new(2, 3).unwrap(),
            ] {
                let expected = cv::resize_linear(view, output).unwrap();
                let actual = cv::resize_gray_roi(&bytes, size, stride, roi, output).unwrap();
                assert_eq!(
                    actual.view().data(),
                    expected.view().data(),
                    "stride {stride}, {roi:?}"
                );
                assert_eq!(bytes, unchanged);
            }
        }
    }
    let actual = cv::resize_gray_roi(
        &[1, 2, 3, 99, 4, 5, 6, 98],
        Dimensions::new(3, 2).unwrap(),
        4,
        Rect {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        },
        Dimensions::new(2, 2).unwrap(),
    )
    .unwrap();
    assert_eq!(actual.view().data(), &[2, 3, 5, 6]);
}

#[test]
fn strided_gray_roi_checks_full_plane_extent_and_bounds_before_native_access() {
    cv::initialize(2).unwrap();
    let size = Dimensions::new(4, 3).unwrap();
    let output = Dimensions::new(2, 2).unwrap();
    let valid = Rect {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    assert!(cv::resize_gray_roi(&[0; 17], size, 6, valid, output).is_err());
    assert!(cv::resize_gray_roi(&[0; 18], size, 3, valid, output).is_err());
    assert!(cv::resize_gray_roi(&[0; 18], size, usize::MAX, valid, output).is_err());
    for roi in [
        Rect { x: -1, ..valid },
        Rect { y: -1, ..valid },
        Rect { width: 0, ..valid },
        Rect { height: 0, ..valid },
        Rect { x: 3, ..valid },
        Rect { y: 2, ..valid },
        Rect {
            width: u32::MAX,
            ..valid
        },
    ] {
        assert!(
            cv::resize_gray_roi(&[0; 18], size, 6, roi, output).is_err(),
            "{roi:?}"
        );
    }
}
