use openpilot_params::Params;

#[test]
fn boolean_methods_preserve_native_exact_byte_semantics() {
    let root = tempfile::tempdir().unwrap();
    let params = Params::open(root.path(), "d").unwrap();
    assert!(!params.get_bool("AlwaysOnDM").unwrap());
    for bytes in [b"".as_slice(), b"0", b"true", b"1\n", b"01"] {
        params.put("AlwaysOnDM", bytes).unwrap();
        assert!(!params.get_bool("AlwaysOnDM").unwrap());
    }
    params.put_bool("AlwaysOnDM", true).unwrap();
    assert!(params.get_bool("AlwaysOnDM").unwrap());
    assert_eq!(params.get("AlwaysOnDM").unwrap().unwrap(), b"1");
    params.put_bool("AlwaysOnDM", false).unwrap();
    assert_eq!(params.get("AlwaysOnDM").unwrap().unwrap(), b"0");
}
