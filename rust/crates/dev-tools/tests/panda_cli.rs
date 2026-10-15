use openpilot_dev_tools::argparse_scan::{scan, OptionSpec};
#[test]
fn scanner_preserves_two_phase_ambiguity_and_missing_values() {
    let spec = [
        OptionSpec::value("--output", true),
        OptionSpec::flag("--safety", Some("mode")),
        OptionSpec::flag("--state", Some("mode")),
    ];
    assert!(scan(&["--help".into(), "--s".into()], &spec).is_err());
    assert!(scan(&["--output".into(), "--help".into()], &spec).is_err());
    assert!(scan(
        &["--safety".into(), "--safety".into(), "--output=x".into()],
        &spec
    )
    .is_ok());
}
#[test]
fn all_original_cli_admissions_and_compiler_boundaries() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/panda-cli/original.json")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let peers = temp.path().join("peers");
    fs::create_dir(&peers).unwrap();
    let script = r#"#!/bin/sh
printf 'call\n' >> "$AUDIT_CALLS"
case "$0" in
*/capnp)
 for arg in "$@"; do case "$arg" in -oc++:*) dir=${arg#-oc++:};; esac; done
 for name in car custom deprecated log; do printf 'controlled generated %s\n' "$name" > "$dir/$name.capnp.c++"; done;;
*)
 while [ "$#" -gt 0 ]; do if [ "$1" = -o ]; then shift; printf 'controlled compiler product\n' > "$1"; break; fi; shift; done;;
esac
"#;
    for name in ["capnp", "clang++", "controlled-clang++"] {
        let p = peers.join(name);
        fs::write(&p, script).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let deps = temp.path().join("deps");
    fs::create_dir_all(deps.join("usb/libusb-1.0")).unwrap();
    fs::write(deps.join("usb/libusb-1.0/libusb.h"), "controlled header\n").unwrap();
    for (i, row) in fixture.as_array().unwrap().iter().enumerate() {
        let family = row["family"].as_str().unwrap();
        let bin = match family {
            "protocol" => env!("CARGO_BIN_EXE_build-pandad-protocol-source"),
            "usb" => env!("CARGO_BIN_EXE_build-pandad-usb-source"),
            "spi" => env!("CARGO_BIN_EXE_build-pandad-spi-source"),
            "peripheral" => env!("CARGO_BIN_EXE_build-pandad-peripheral-source"),
            _ => env!("CARGO_BIN_EXE_build-pandad-runtime-fixture"),
        };
        let cwd = temp.path().join(i.to_string());
        fs::create_dir(&cwd).unwrap();
        let output = cwd.join("out");
        let calls = cwd.join("calls");
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap()
                    .replace("$OUT", output.to_str().unwrap())
                    .replace("$DEPS", deps.to_str().unwrap())
            })
            .collect::<Vec<_>>();
        let result = Command::new(bin)
            .args(args)
            .current_dir(&cwd)
            .env(
                "PATH",
                format!("{}:{}", peers.display(), std::env::var("PATH").unwrap()),
            )
            .env("AUDIT_CALLS", &calls)
            .output()
            .unwrap();
        assert_eq!(
            result.status.code().unwrap() as i64,
            row["exit"].as_i64().unwrap(),
            "{family} {}: {}",
            row["case"],
            String::from_utf8_lossy(&result.stderr)
        );
        let actual = fs::read_to_string(calls)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(
            actual as u64,
            row["calls"].as_u64().unwrap(),
            "{family} {}",
            row["case"]
        );
        if row["exit"] == 2 {
            assert_eq!(
                fs::read_dir(&cwd).unwrap().count(),
                0,
                "parse error must precede filesystem mutation: {row}"
            );
        }
    }
}
#[test]
fn scanner_preserves_negative_decimal_values_and_action_order() {
    let specs = [
        OptionSpec::value("--output", true),
        OptionSpec::flag("--safety", Some("mode")),
        OptionSpec::flag("--state", Some("mode")),
    ];
    for value in ["-١", "-.١", "-1.2", "-", "-1\n", "-unknown value"] {
        let got = scan(&["--output".into(), value.into()], &specs).unwrap();
        assert_eq!(got.values["--output"], value);
    }
    for value in ["-²", "-①", "--", "--safety"] {
        assert!(scan(&["--output".into(), value.into()], &specs).is_err());
    }
    assert!(scan(
        &["--safety".into(), "--state".into(), "--help".into()],
        &specs
    )
    .is_err());
    assert!(
        scan(
            &["--help".into(), "--safety".into(), "--state".into()],
            &specs
        )
        .unwrap()
        .help
    );
    assert!(
        scan(&["--unknown".into(), "--help".into()], &specs)
            .unwrap()
            .help
    );
    assert!(scan(&["--help=yes".into()], &specs).is_err());
    assert!(scan(&["--safety=yes".into(), "--help".into()], &specs).is_err());
    assert_eq!(
        scan(&["--output=a".into(), "--o=b".into()], &specs)
            .unwrap()
            .values["--output"],
        "b"
    );
    assert!(scan(&["--output=a".into(), "--".into(), "--help".into()], &specs).is_err());
}
#[test]
fn positional_mode_preserves_literal_terminator_and_deferred_extras() {
    use openpilot_dev_tools::argparse_scan::scan_with_positionals;
    let specs = [OptionSpec::value("--root", false)];
    let literal = scan_with_positionals(&["--".into(), "--help".into()], &specs, 1).unwrap();
    assert!(!literal.help);
    assert_eq!(literal.positionals, ["--help"]);
    assert!(scan_with_positionals(&["out".into(), "extra".into()], &specs, 1).is_err());
    assert!(
        scan_with_positionals(&["out".into(), "extra".into(), "--help".into()], &specs, 1)
            .unwrap()
            .help
    );
    assert!(scan_with_positionals(&["out".into(), "--".into()], &specs, 1).is_ok());
    assert!(scan(&["--".into(), "--help".into()], &specs).is_err());
}
