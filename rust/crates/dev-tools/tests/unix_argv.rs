use openpilot_dev_tools::{
    argparse_scan::{scan_os, OptionSpec},
    qa_process::{CommandSpec, NativeRunner, Runner},
};
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    time::Duration,
};
#[test]
fn authoritative_raw_argv_reaches_actual_subprocess_unchanged() {
    let args = vec![
        OsString::from("/bin/sh"),
        "-c".into(),
        "printf '%s\\0' \"$@\"".into(),
        "peer".into(),
        OsString::from_vec(b"path-\xff".to_vec()),
        "Unicode-한".into(),
    ];
    let mut spec = CommandSpec::new_os(args, Duration::from_secs(2));
    spec.argv = vec!["/must-not-run-diagnostic-mirror".into()];
    let result = NativeRunner.run(&spec).unwrap();
    assert_eq!(result.code, 0);
    assert_eq!(result.stdout, b"path-\xff\0Unicode-\xed\x95\x9c\0");
}
#[test]
fn lossless_parser_preserves_inline_raw_paths_and_marker_collisions() {
    let raw = OsString::from_vec(b"-space \xff".to_vec());
    let args = vec![
        "--output=old".into(),
        "--out".into(),
        raw.clone(),
        "--other".into(),
        "\u{e000}0\u{e001}".into(),
    ];
    let parsed = scan_os(
        &args,
        &[
            OptionSpec::value("--output", true),
            OptionSpec::value("--other", true),
        ],
    )
    .unwrap();
    assert_eq!(parsed.values["--output"].as_bytes(), raw.as_bytes());
    assert_eq!(parsed.values["--other"], "\u{e000}0\u{e001}");
    let inline = OsString::from_vec(b"--output=raw-\xff".to_vec());
    let parsed = scan_os(&[inline], &[OptionSpec::value("--output", true)]).unwrap();
    assert_eq!(parsed.values["--output"].as_bytes(), b"raw-\xff");
}
#[test]
fn original_raw_path_argument_grammar_matches() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/ui-adapter-sanitizers/raw-parser.json"
    ))
    .unwrap();
    fn decode(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .chunks_exact(2)
            .map(|s| u8::from_str_radix(std::str::from_utf8(s).unwrap(), 16).unwrap())
            .collect()
    }
    for row in rows.as_array().unwrap() {
        let args = row["args_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| OsString::from_vec(decode(s.as_str().unwrap())))
            .collect::<Vec<_>>();
        let result = scan_os(
            &args,
            &[
                OptionSpec::value("--target", true),
                OptionSpec::value("--raylib", true),
                OptionSpec::value("--evidence", true),
            ],
        );
        assert_eq!(result.is_ok(), row["exit"] == 0, "{row}");
        if let Ok(parsed) = result {
            assert_eq!(parsed.help, row["help"].as_bool().unwrap(), "{row}");
            if !parsed.help {
                for (key, value) in parsed.values {
                    let name = key.trim_start_matches("--");
                    assert_eq!(
                        value.as_bytes(),
                        decode(row["parsed"][name].as_str().unwrap()),
                        "{row}"
                    );
                }
            }
        }
    }
}
#[test]
fn authoritative_empty_and_nul_argv_are_rejected() {
    let mut empty = CommandSpec::new_os(vec![], Duration::from_secs(2));
    empty.argv = vec!["/bin/true".into()];
    assert!(NativeRunner.run(&empty).is_err());
    let nul = CommandSpec::new_os(
        vec!["/bin/true".into(), OsString::from_vec(b"bad\0arg".to_vec())],
        Duration::from_secs(2),
    );
    assert!(NativeRunner.run(&nul).is_err());
}
