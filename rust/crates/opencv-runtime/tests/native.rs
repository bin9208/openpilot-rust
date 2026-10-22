#![cfg(feature = "native-skip-miri")]
use openpilot_opencv_runtime::{
    self as cv, Dimensions, DnnNet, Format, ImageLayout, ImageView, Point, TensorView,
};
use std::{env, path::Path};

#[test]
fn padded_nv12_planes_match_packed_pixels_without_touching_padding() {
    cv::initialize(2).unwrap();
    let dimensions = Dimensions::new(4, 4).unwrap();
    let packed: Vec<u8> = (0..24).map(|i| (i * 19) as u8).collect();
    let expected = cv::nv12_to_rgb(&packed, dimensions).unwrap().into_data();
    for stride in [4, 6, 16] {
        let offset = stride * 4 + 8;
        let mut raw = vec![0xff; offset + stride * 2];
        for row in 0..4 {
            raw[row * stride..row * stride + 4].copy_from_slice(&packed[row * 4..row * 4 + 4]);
        }
        for row in 0..2 {
            raw[offset + row * stride..offset + row * stride + 4]
                .copy_from_slice(&packed[16 + row * 4..20 + row * 4]);
        }
        let before = raw.clone();
        let actual = cv::nv12_planes_to_rgb(&raw, dimensions, stride, offset).unwrap();
        assert_eq!(actual.view().data(), expected);
        assert_eq!(raw, before);
        assert!(cv::nv12_planes_to_rgb(&raw[..raw.len() - 1], dimensions, stride, offset).is_err());
        assert!(cv::nv12_planes_to_rgb(&raw, dimensions, 3, offset).is_err());
        assert!(cv::nv12_planes_to_rgb(&raw, dimensions, stride, stride * 4 - 1).is_err());
    }
}

#[test]
fn once_only_threads_and_external_errors_are_observable_without_unwinding() {
    assert!(cv::initialize(0).is_err());
    cv::initialize(2).unwrap();
    cv::initialize(2).unwrap();
    assert!(cv::initialize(1).is_err());
    assert!(DnnNet::load(Path::new("/definitely-missing-model.onnx")).is_err());
    let path = Path::new("/model\0unexpected.onnx");
    assert!(DnnNet::load(path).is_err());
}

#[test]
fn image_bounds_masks_lengths_and_outputs_survive_original_input_drop() {
    cv::initialize(2).unwrap();
    let dimensions = Dimensions::new(4, 2).unwrap();
    let layout = ImageLayout::new(dimensions, Format::Rgb).unwrap();
    let resized = {
        let bytes = vec![42; layout.len()];
        cv::resize_linear(
            ImageView::new(&bytes, layout).unwrap(),
            Dimensions::new(2, 1).unwrap(),
        )
        .unwrap()
    };
    assert_eq!(resized.view().data(), &[42; 6]);
    let polygon = [
        Point { x: -1, y: -1 },
        Point { x: 3, y: 0 },
        Point { x: 1, y: 2 },
    ];
    let bounds = cv::bounding_rect(&polygon).unwrap();
    assert_eq!(
        (bounds.x, bounds.y, bounds.width, bounds.height),
        (-1, -1, 5, 4)
    );
    let mask = cv::polygon_mask(dimensions, &polygon).unwrap();
    assert!(cv::apply_mask(resized.view(), mask.view()).is_err());
    assert!(cv::nv12_to_rgb(&[0; 11], dimensions).is_err());
    assert!(
        cv::bounding_rect(&[Point { x: i32::MIN, y: 0 }, Point { x: i32::MAX, y: 0 }]).is_err()
    );
}

#[test]
fn real_model_tensors_are_owned_across_input_release_and_next_forward() {
    cv::initialize(2).unwrap();
    let path = env::var("OPENCV_TEST_LANE_MODEL").expect("real pinned lane ONNX fixture path");
    let mut net = DnnNet::load(Path::new(&path)).unwrap();
    let names = net.output_names().unwrap();
    assert_eq!(names, ["output0", "output1"]);
    let shape = [1, 3, 416, 416];
    let first = {
        let values = vec![0.5; 3 * 416 * 416];
        net.forward(TensorView::new(&shape, &values).unwrap(), &names)
            .unwrap()
    };
    let first_bits: Vec<Vec<u32>> = first
        .iter()
        .map(|tensor| tensor.view().values().iter().map(|v| v.to_bits()).collect())
        .collect();
    let original = first[0].view().values().to_vec();
    let values = vec![0.; 3 * 416 * 416];
    let next = net
        .forward(TensorView::new(&shape, &values).unwrap(), &names)
        .unwrap();
    assert_eq!(first[0].view().values(), original);
    assert_eq!(first[0].view().shape(), next[0].view().shape());
    assert!(net
        .forward(
            TensorView::new(&shape, &values).unwrap(),
            &["missing-output".into()]
        )
        .is_err());
    // Re-enter after an exception with fresh input storage. Both network outputs
    // and previously returned outputs must retain exact bits across the failed
    // call, dropped input and intervening inference on a different tensor.
    let recovered = {
        let mut values = vec![0.5; 3 * 416 * 416];
        let result = net
            .forward(TensorView::new(&shape, &values).unwrap(), &names)
            .unwrap();
        values.fill(f32::NAN);
        result
    };
    for (output, expected) in recovered
        .iter()
        .chain(&first)
        .zip(first_bits.iter().cycle())
    {
        assert_eq!(
            output
                .view()
                .values()
                .iter()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            *expected
        );
    }
}
