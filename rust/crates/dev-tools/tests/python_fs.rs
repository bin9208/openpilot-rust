use openpilot_dev_tools::{argparse_scan::scan_os_with_positionals, python_fs};
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
};
#[test]
fn filesystem_json_preserves_surrogateescape_without_combining_raw_pairs() {
    let path = OsString::from_vec(b"file-\xff-\xed\xa0\x80-\xed\xb0\x80".to_vec());
    assert_eq!(
        python_fs::json_string(&path).unwrap(),
        r#""file-\udcff-\udced\udca0\udc80-\udced\udcb0\udc80""#
    );
    assert_eq!(
        python_fs::json_value(&path).to_json().unwrap(),
        python_fs::json_string(&path).unwrap()
    );
}
#[test]
fn positional_raw_arguments_keep_terminator_literals_and_help_order() {
    let raw = OsString::from_vec(b"--archive-\xff".to_vec());
    let result =
        scan_os_with_positionals(&["--".into(), raw.clone(), "output".into()], &[], 2).unwrap();
    assert_eq!(result.positionals[0].as_bytes(), raw.as_bytes());
    assert!(
        scan_os_with_positionals(&[raw, "--help".into()], &[], 2)
            .unwrap()
            .help
    );
}
#[test]
fn filesystem_json_decodes_all_bytes_and_unicode_without_replacement() {
    for bytes in [
        (0..=255).collect::<Vec<u8>>(),
        "prefix-한글-🦀".as_bytes().to_vec(),
        b"raw-\xed\xa0\x80-\xed\xb0\x80".to_vec(),
    ] {
        let path = OsString::from_vec(bytes);
        assert_eq!(
            python_fs::from_json(&python_fs::json_value(&path)).unwrap(),
            path
        );
    }
}
#[test]
fn filesystem_json_rejects_non_text_unencodable_and_noncanonical_strings() {
    use openpilot_log_format::JsonValue;
    for value in [
        JsonValue::parse("1").unwrap(),
        JsonValue::codepoints(vec![0xd800]).unwrap(),
        JsonValue::codepoints(vec![0xdc7f]).unwrap(),
        // These bytes decode as a single U+0080, not two escaped raw bytes.
        JsonValue::codepoints(vec![0xdcc2, 0xdc80]).unwrap(),
    ] {
        assert!(python_fs::from_json(&value).is_err());
    }
}
