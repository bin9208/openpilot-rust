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
