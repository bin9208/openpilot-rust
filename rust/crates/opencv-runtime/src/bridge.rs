#[cxx::bridge(namespace = "openpilot_opencv")]
pub(crate) mod ffi {
    struct Dimensions {
        width: u32,
        height: u32,
    }
    struct Layout {
        size: Dimensions,
        channels: u8,
    }
    struct Point {
        x: i32,
        y: i32,
    }
    struct Rect {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    }
    struct Tensor {
        shape: Vec<i32>,
        values: Vec<f32>,
    }

    // SAFETY: adapters check dimensions/products against slices before OpenCV,
    // do not mutate borrowed input, retain only an owned setInput clone, and copy
    // all results into owned Rust vectors. CXX translates C++ exceptions to Result.
    unsafe extern "C++" {
        include!("bridge.h");
        type Net;
        fn set_threads(count: i32) -> Result<()>;
        fn resize(data: &[u8], layout: Layout, output: Dimensions) -> Result<Vec<u8>>;
        fn bgr_gray(data: &[u8], size: Dimensions) -> Result<Vec<u8>>;
        fn nv12_rgb(data: &[u8], size: Dimensions) -> Result<Vec<u8>>;
        fn mask_polygon(points: &[Point], size: Dimensions) -> Result<Vec<u8>>;
        fn bounds(points: &[Point]) -> Result<Rect>;
        fn mask_image(data: &[u8], mask: &[u8], layout: Layout) -> Result<Vec<u8>>;
        fn load_onnx(path: &str) -> Result<UniquePtr<Net>>;
        fn output_names(net: &Net) -> Result<Vec<String>>;
        fn forward(
            net: Pin<&mut Net>,
            input: &[f32],
            shape: &[i32],
            names: &[String],
        ) -> Result<Vec<Tensor>>;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;

    #[test]
    fn cpp_validates_borrowed_lengths_and_layouts_independently() {
        let valid = || ffi::Dimensions {
            width: 4,
            height: 2,
        };
        assert!(ffi::resize(
            &[0; 7],
            ffi::Layout {
                size: valid(),
                channels: 1
            },
            valid()
        )
        .is_err());
        assert!(ffi::resize(
            &[],
            ffi::Layout {
                size: valid(),
                channels: 2
            },
            valid()
        )
        .is_err());
        assert!(ffi::nv12_rgb(&[0; 11], valid()).is_err());
        assert!(ffi::nv12_rgb(
            &[0; 12],
            ffi::Dimensions {
                width: 3,
                height: 2
            }
        )
        .is_err());
        assert!(ffi::mask_image(
            &[0; 24],
            &[0; 7],
            ffi::Layout {
                size: valid(),
                channels: 3
            }
        )
        .is_err());
        assert!(ffi::bounds(&[]).is_err());
        assert!(ffi::bounds(&[
            ffi::Point { x: i32::MIN, y: 0 },
            ffi::Point { x: i32::MAX, y: 0 }
        ])
        .is_err());
        assert!(ffi::mask_polygon(&[ffi::Point { x: 1, y: 1 }], valid()).is_err());
    }

    #[test]
    fn cpp_checks_tensor_shape_before_touching_a_net_input() {
        crate::initialize(2).unwrap();
        let path = std::env::var("OPENCV_TEST_LANE_MODEL").expect("real lane ONNX path");
        let mut net = ffi::load_onnx(&path).unwrap();
        assert!(ffi::forward(net.pin_mut(), &[], &[], &[]).is_err());
        assert!(ffi::forward(net.pin_mut(), &[], &[1, -3, 2], &[]).is_err());
        assert!(ffi::forward(net.pin_mut(), &[], &[i32::MAX; 32], &[]).is_err());
        assert!(ffi::forward(net.pin_mut(), &[0.; 3], &[1, 4], &[]).is_err());
    }
}
