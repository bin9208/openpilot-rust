use openpilot_dev_tools::image_array::{decode, pixel_warning, MAX_IMAGE_PIXELS};
use sha2::{Digest, Sha256};

#[test]
fn captured_pillow_numpy_modes_shapes_and_integer_samples() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image-array");
    for manifest in [
        "original.json",
        "edges.json",
        "tiff-types.json",
        "legacy.json",
        "legacy-extra.json",
        "simple-formats.json",
        "remaining-formats.json",
        "extended-formats.json",
        "last-families.json",
        "blp-actual.json",
        "review-regressions.json",
    ] {
        let captured: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(manifest)).unwrap()).unwrap();
        let cases = captured
            .as_array()
            .or_else(|| captured["cases"].as_array())
            .unwrap();
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let bytes = std::fs::read(root.join(format!("{name}.image"))).unwrap();
            if case.get("error").is_some() {
                assert!(decode(&bytes).is_err(), "{name} must fail as Pillow does");
                continue;
            }
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                case["input_sha256"].as_str().unwrap(),
                "{name} input"
            );
            let array = decode(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(array.mode, case["mode"].as_str().unwrap(), "{name} mode");
            assert_eq!(
                serde_json::json!(array.shape),
                case["shape"],
                "{name} shape"
            );
            let mut digest = Sha256::new();
            for value in array.values {
                digest.update(value.to_le_bytes());
            }
            assert_eq!(
                format!("{:x}", digest.finalize()),
                case["values_sha256"].as_str().unwrap(),
                "{name} values"
            );
        }
    }
}

#[test]
fn truncated_jpeg_acceptance_matches_pillow() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image-array");
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("jpeg-errors.json")).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        assert_eq!(
            decode(&std::fs::read(root.join(format!("{name}.image"))).unwrap()).is_err(),
            case["error"].as_bool().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn pillow_pixel_thresholds_are_checked_without_allocating_images() {
    for size in [MAX_IMAGE_PIXELS - 1, MAX_IMAGE_PIXELS] {
        assert!(pixel_warning(size, 1).unwrap().is_none());
    }
    for size in [MAX_IMAGE_PIXELS + 1, MAX_IMAGE_PIXELS * 2] {
        assert!(pixel_warning(size, 1).unwrap().is_some());
    }
    assert!(pixel_warning(MAX_IMAGE_PIXELS * 2 + 1, 1).is_err());
    assert!(pixel_warning(0, 1).is_err());
    assert!(pixel_warning(1, 0).is_err());
    assert!(pixel_warning(usize::MAX, 2).is_err());
}

#[test]
fn malformed_inputs_return_errors_without_panicking() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image-array");
    for name in [
        "RGB-SGI", "RGB-IM", "RGB-DDS", "P-PCX", "L-BMP", "P-TGA", "RGB-TIFF", "RGB-PNG",
        "RGB-ICNS",
    ] {
        let bytes = std::fs::read(root.join(format!("{name}.image"))).unwrap();
        for end in [0, 1, 4, 17, bytes.len() / 2] {
            let result = std::panic::catch_unwind(|| decode(&bytes[..end.min(bytes.len())]));
            assert!(result.is_ok(), "{name} truncated at {end} panicked");
        }
    }
}

#[cfg(feature = "image-array-native")]
#[test]
fn native_sdk_formats_use_the_production_array_path() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/image-array");
    for manifest in [
        "native-formats.json",
        "native-errors.json",
        "native-variants.json",
        "embedded-native.json",
        "compression-tiff.json",
        "tiff-openinfo.json",
        "tiff-malformed.json",
    ] {
        let cases: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(manifest)).unwrap()).unwrap();
        for case in cases.as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            // A failed encoder produced no input and says nothing about decoder admission.
            if case.get("save_error").is_some() {
                continue;
            }
            let bytes = std::fs::read(root.join(format!("{name}.image"))).unwrap();
            if case.get("error").is_some() {
                assert!(decode(&bytes).is_err(), "{name} original rejection");
                continue;
            }
            assert_eq!(
                format!("{:x}", Sha256::digest(&bytes)),
                case["input_sha256"].as_str().unwrap(),
                "{name} input"
            );
            let image = decode(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(image.mode, case["mode"].as_str().unwrap(), "{name} mode");
            assert_eq!(
                serde_json::json!(image.shape),
                case["shape"],
                "{name} shape"
            );
            let mut digest = Sha256::new();
            for value in image.values {
                digest.update(value.to_le_bytes());
            }
            assert_eq!(
                format!("{:x}", digest.finalize()),
                case["values_sha256"].as_str().unwrap(),
                "{name} every integer sample"
            );
        }
    }
}
