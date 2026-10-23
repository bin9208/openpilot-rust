#![cfg(feature = "native-skip-miri")]

#[path = "support/preprocessing_dc951380.rs"]
mod baseline;

use openpilot_opencv_runtime::{self as cv, Dimensions};
use openpilot_xiaoge::{config, nv12, vision};
use openpilot_xiaoge::{
    config::Config,
    inference::{lane_image, lane_tensor, Geometry},
    nv12::{Frame, Layout},
    vision::Side,
};
use serde_json::json;

#[test]
fn lane_roi_pixels_and_tensor_bits_preserve_packed_center_crop() {
    cv::initialize(2).unwrap();
    // Both odd differences floor the center origin; odd Y stride is supported.
    // Lane only requires the Y allocation, with no UV validation or chroma use.
    for (width, height, stride) in [
        (9, 6, 11),
        (6, 9, 7),
        (7, 7, 9),
        (1, 3, 1),
        (419, 416, 421),
        (416, 419, 417),
        (1928, 1208, 1984),
    ] {
        let data: Vec<_> = (0..stride * height).map(|i| (i * 73 % 256) as u8).collect();
        let frame = Frame::new(
            &data,
            Layout {
                width,
                height,
                stride,
                uv_offset: 0,
            },
        )
        .unwrap();
        let before = baseline::lane_image(&frame).unwrap();
        let after = lane_image(&frame).unwrap();
        assert_eq!(after.view().layout(), before.view().layout());
        assert_eq!(
            after.view().data(),
            before.view().data(),
            "{width}x{height}, stride {stride}"
        );
        let before = lane_tensor(&before).unwrap();
        let after = lane_tensor(&after).unwrap();
        assert_eq!(after.view().shape(), before.view().shape());
        assert!(after
            .view()
            .values()
            .iter()
            .zip(before.view().values())
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }
}

#[test]
fn composite_bsd_tensor_preserves_padding_chroma_mask_and_letterbox_bits() {
    cv::initialize(2).unwrap();
    for (width, height, stride, gap) in [(20, 12, 23, 5), (12, 20, 16, 8), (20, 20, 20, 0)] {
        let dimensions = Dimensions::new(width as u32, height as u32).unwrap();
        let config = Config::normalize(&json!({"width":width,"height":height,
            "poly_left":[[-2,1],[9,2],[5,11]],
            "poly_right":[[10,0],[30,2],[18,20],[12,9]]}))
        .unwrap();
        let geometry = Geometry::new(&config, dimensions).unwrap();
        let previous = baseline::geometry::Geometry::new(&config, dimensions).unwrap();
        assert_eq!(geometry.dimensions(), previous.dimensions());
        let layout = Layout {
            width,
            height,
            stride,
            uv_offset: stride * height + gap,
        };
        let end = layout.uv_offset + stride * height / 2;
        let data: Vec<_> = (0..end + 7).map(|i| (i * 47 % 256) as u8).collect();
        let frame = Frame::new(&data, layout).unwrap();
        for side in [Side::Left, Side::Right] {
            assert_eq!(
                geometry.region(side).unwrap().bounds(),
                previous.region(side).unwrap().bounds()
            );
            assert_eq!(
                geometry.region(side).unwrap().mask().view().data(),
                previous.region(side).unwrap().mask().view().data()
            );
            let before = previous.tensor(&frame, side).unwrap().unwrap();
            let after = geometry.tensor(&frame, side).unwrap().unwrap();
            assert_eq!(after.view().shape(), before.view().shape());
            assert!(
                after
                    .view()
                    .values()
                    .iter()
                    .zip(before.view().values())
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "{width}x{height}, {side:?}"
            );
        }
        // A selected crop fitting in memory must not bypass full UV validation.
        for short in [layout.uv_offset, end - 1] {
            let frame = Frame::new(&data[..short], layout).unwrap();
            assert!(geometry.tensor(&frame, Side::Left).is_err());
        }
        let overlapping = Frame::new(
            &data,
            Layout {
                uv_offset: stride * height - 1,
                ..layout
            },
        )
        .unwrap();
        assert!(geometry.tensor(&overlapping, Side::Left).is_err());
        let overflow = Frame::new(
            &data,
            Layout {
                uv_offset: usize::MAX - 1,
                ..layout
            },
        )
        .unwrap();
        assert!(geometry.tensor(&overflow, Side::Left).is_err());
    }
}
