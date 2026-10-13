use openpilot_cluster::runtime::output::Geometry;
use openpilot_cluster::runtime_args::{Codec, H264Backend, RuntimeArgs};

#[test]
fn actual_landscape_metadata_preserves_logical_crop_before_hardware_alignment() {
    let args = RuntimeArgs {
        usb_codec: Codec::H264,
        usb_h264_backend: H264Backend::Native,
        ..RuntimeArgs::default()
    };
    let geometry = Geometry::resolve(&args, Some([1920, 462]), true).unwrap();
    assert_eq!(geometry.frame, [1920, 462]);
    assert_eq!(geometry.logical, [462, 1920]);
    assert_eq!(geometry.encoder, [464, 1920]);
    let software = Geometry::resolve(&args, Some([1920, 462]), false).unwrap();
    assert_eq!(software.encoder, [462, 1920]);
    let error = Geometry::resolve(&args, Some([1920, 463]), true).unwrap_err();
    assert_eq!(
        error.to_string(),
        "H264 USB output requires even encoder dimensions, got 463x1920"
    );
}
