fn copy_bytes(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}
fn copy_labels(data: &[i32]) -> Vec<i32> {
    data.to_vec()
}
fn copy_scores(data: &[f32]) -> Vec<f32> {
    data.to_vec()
}

#[cxx::bridge(namespace = "openpilot_opencv")]
pub(crate) mod ffi {
    extern "Rust" {
        fn copy_bytes(data: &[u8]) -> Vec<u8>;
        fn copy_labels(data: &[i32]) -> Vec<i32>;
        fn copy_scores(data: &[f32]) -> Vec<f32>;
    }
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

    struct Component {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        area: i32,
        center_x: f64,
        center_y: f64,
    }
    struct Components {
        labels: Vec<i32>,
        stats: Vec<Component>,
    }
    struct Contour {
        bounds: Rect,
        area: f64,
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
        fn rgb_gray(data: &[u8], size: Dimensions) -> Result<Vec<u8>>;
        fn rgb_hsv(data: &[u8], size: Dimensions) -> Result<Vec<u8>>;
        fn components(data: &[u8], size: Dimensions) -> Result<Components>;
        fn morphology_open(data: &[u8], size: Dimensions, kernel: Dimensions) -> Result<Vec<u8>>;
        fn morphology_tophat(data: &[u8], size: Dimensions, kernel: Dimensions) -> Result<Vec<u8>>;
        fn distance_transform(data: &[u8], size: Dimensions) -> Result<Vec<f32>>;
        fn external_contours(data: &[u8], size: Dimensions) -> Result<Vec<Contour>>;
        fn match_template(
            data: &[u8],
            size: Dimensions,
            template_data: &[u8],
            template_size: Dimensions,
            mask: &[u8],
        ) -> Result<Vec<f32>>;
        fn match_template_ccorr(
            data: &[u8],
            size: Dimensions,
            template_data: &[u8],
            template_size: Dimensions,
            mask: &[u8],
        ) -> Result<Vec<f32>>;
        fn nv12_rgb(data: &[u8], size: Dimensions) -> Result<Vec<u8>>;
        fn nv12_planes_rgb(
            data: &[u8],
            size: Dimensions,
            stride: usize,
            uv_offset: usize,
        ) -> Result<Vec<u8>>;
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
    fn signal_primitives_reject_inconsistent_borrowed_extents() {
        let size = || ffi::Dimensions {
            width: 4,
            height: 2,
        };
        assert!(ffi::rgb_gray(&[0; 23], size()).is_err());
        assert!(ffi::rgb_hsv(&[0; 23], size()).is_err());
        assert!(ffi::components(&[0; 7], size()).is_err());
        assert!(ffi::morphology_open(
            &[0; 8],
            size(),
            ffi::Dimensions {
                width: 0,
                height: 1
            }
        )
        .is_err());
        assert!(ffi::match_template(
            &[0; 8],
            size(),
            &[0; 3],
            ffi::Dimensions {
                width: 2,
                height: 2
            },
            &[]
        )
        .is_err());
        assert!(ffi::match_template(
            &[0; 8],
            size(),
            &[0; 4],
            ffi::Dimensions {
                width: 2,
                height: 2
            },
            &[0; 3]
        )
        .is_err());
        let pixels = [0_u8; 8];
        let _ = ffi::external_contours(&pixels, size()).unwrap();
        assert_eq!(pixels, [0; 8]);
    }

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
