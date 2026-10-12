use openpilot_usbgpu::worker::{Info, Metadata};
use serde_json::json;

fn metadata() -> Metadata {
    serde_json::from_value(json!({"model_sha256":"0".repeat(64),"checkpoint":"source-local",
        "inputs":[{"name":"desire","shape":[8]},{"name":"traffic_convention","shape":[1,2]},
            {"name":"action_t","shape":[1,2]}],"output_count":8,"output_slices":{"hidden_state":[4,8,null]}})).unwrap()
}
#[test]
fn local_full_camera_buffer_rejects_plane_only_frames() {
    for (camera, planes, full) in [
        ([1344, 760], 1622016, 2428928),
        ([1928, 1208], 3735552, 4804608),
    ] {
        let generic = Info::new(metadata(), camera).unwrap();
        let local = Info::local(metadata(), camera).unwrap();
        assert_eq!(generic.frame_size, planes);
        assert_eq!(local.frame_size, full);
        assert_eq!(local.input_shapes["img"], [full]);
        assert_eq!(local.input_shapes["big_img"], [full]);
        let first = local.layout["img"].offset;
        assert!(local.input(&vec![0; first + planes], "img").is_err());
        assert_eq!(
            local.input(&vec![0; first + full], "img").unwrap().len(),
            full
        );
    }
}
