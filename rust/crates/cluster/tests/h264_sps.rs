use openpilot_cluster::h264::sps::{patch_constraints, patch_crop, patch_vui};

#[test]
fn baseline_constraint_patch_preserves_start_codes_and_non_sps_nals() {
    let bytes = [
        0, 0, 1, 0x67, 0x42, 0, 0x1f, 0, 0, 0, 1, 0x68, 0x42, 0, 0x1f,
    ];
    let (patched, changed) = patch_constraints(&bytes);
    assert!(changed);
    assert_eq!(
        patched,
        [0, 0, 1, 0x67, 0x42, 0x40, 0x1f, 0, 0, 0, 1, 0x68, 0x42, 0, 0x1f]
    );
    assert_eq!(patch_constraints(&patched), (patched.clone(), false));
}

#[test]
fn malformed_sps_is_retained_without_fabricating_crop_or_timing() {
    for bytes in [
        vec![],
        vec![0, 0, 1, 0x67],
        vec![0, 0, 1, 0x67, 0xff, 0, 0, 0],
    ] {
        assert_eq!(
            patch_crop(&bytes, 480, 1920),
            (bytes.clone(), false, String::new())
        );
        assert_eq!(patch_vui(&bytes, 10), (bytes.clone(), false, String::new()));
    }
}
