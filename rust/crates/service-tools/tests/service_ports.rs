use openpilot_service_tools::{catalog_ports, fnv1a, port, render};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
#[test]
fn all_eighty_eight_original_service_rows_and_real_cli_bytes_match() {
    let expected = include_str!("fixtures/ports.txt");
    assert_eq!(expected.lines().count(), 88);
    assert_eq!(catalog_ports(), expected);
    let output = Command::new(env!("CARGO_BIN_EXE_list-service-ports"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, expected.as_bytes());
}
#[test]
fn original_fnv_vectors_cover_utf8_empty_nul_and_wrapping_long_names() {
    for row in include_str!("fixtures/fnv.tsv").lines() {
        let values = row.split('\t').collect::<Vec<_>>();
        let bytes = values[0]
            .as_bytes()
            .chunks_exact(2)
            .map(|hex| u8::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let name = String::from_utf8(bytes).unwrap();
        assert_eq!(fnv1a(&name), values[1].parse::<u64>().unwrap(), "{name:?}");
        assert_eq!(port(&name), values[2].parse::<u16>().unwrap(), "{name:?}");
    }
}
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "service-tools-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[cfg(unix)]
#[test]
fn actual_adb_shell_preserves_all_original_argv_and_failures_without_python() {
    use std::os::unix::fs::PermissionsExt;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    for case in [
        "success",
        "occupied-ports",
        "first-service-adb-fails",
        "last-service-adb-fails",
        "ssh-forward-fails",
        "ssh-fails",
        "ss-fails",
        "producer-fails",
    ] {
        let temp = Temp::new();
        let base = &temp.0;
        let bin = base.join("bin");
        fs::create_dir(&bin).unwrap();
        let programs=[
          ("adb", "#!/bin/bash\nprintf '%s\\0' \"$@\" >> \"$CAPTURE/adb.calls\"\ncount=0; if [ -f \"$CAPTURE/adb-count\" ]; then read -r count < \"$CAPTURE/adb-count\" || true; fi\ncount=$((count+1)); printf '%s\\n' \"$count\" > \"$CAPTURE/adb-count\"\nif { [ \"$CASE\" = first-service-adb-fails ] && [ \"$count\" = 1 ]; } || { [ \"$CASE\" = last-service-adb-fails ] && [ \"$count\" = 88 ]; } || { [ \"$CASE\" = ssh-forward-fails ] && [ \"$3\" = tcp:22 ]; }; then exit 13; fi\nprintf 'fake adb stdout\\n'\n"),
          ("ss", "#!/bin/bash\nprintf '%s\\0' \"$@\" >> \"$CAPTURE/ss.calls\"\nif [ \"$CASE\" = ss-fails ]; then exit 17; fi\nif [ \"$CASE\" = occupied-ports ]; then printf 'LISTEN 0 1 127.0.0.1:2222 x\\nLISTEN 0 1 127.0.0.1:2223 x\\n'; fi\n"),
          ("ssh", "#!/bin/bash\nprintf '%s\\0' \"$@\" >> \"$CAPTURE/ssh.calls\"\nprintf 'fake ssh stdout\\n'\nif [ \"$CASE\" = ssh-fails ]; then exit 19; fi\n"),
          ("producer-failed", "#!/bin/bash\nprintf 'controlled producer failure\\n' >&2\nexit 51\n"),
        ];
        for (name, body) in programs {
            let path = bin.join(name);
            fs::write(&path, body).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        for name in ["grep", "dirname", "bash"] {
            let location = Command::new("/bin/sh")
                .args(["-c", &format!("command -v {name}")])
                .output()
                .unwrap();
            let location = String::from_utf8(location.stdout).unwrap();
            std::os::unix::fs::symlink(location.trim(), bin.join(name)).unwrap();
        }
        let producer = if case == "producer-fails" {
            bin.join("producer-failed")
        } else {
            PathBuf::from(env!("CARGO_BIN_EXE_list-service-ports"))
        };
        let result = Command::new("/bin/bash")
            .arg(source.join("tools/scripts/adb_ssh.sh"))
            .args(["echo", "space value", "--flag", "literal$(not-executed)"])
            .env("PATH", &bin)
            .env("CAPTURE", base)
            .env("CASE", case)
            .env("OPENPILOT_SERVICE_PORTS", producer)
            .output()
            .unwrap();
        let expected = fixture().join(case);
        assert_eq!(
            result.status.code().unwrap().to_string(),
            fs::read_to_string(expected.join("status")).unwrap(),
            "{case}"
        );
        assert_eq!(
            result.stdout,
            fs::read(expected.join("stdout")).unwrap(),
            "{case}"
        );
        assert_eq!(
            result.stderr,
            fs::read(expected.join("stderr")).unwrap(),
            "{case}"
        );
        for (actual, key) in [
            ("adb.calls", "adb_argv"),
            ("ss.calls", "ss_argv"),
            ("ssh.calls", "ssh_argv"),
        ] {
            let bytes = fs::read(base.join(actual)).unwrap_or_default();
            assert_eq!(
                bytes,
                fs::read(expected.join(key)).unwrap(),
                "{case}: {key}"
            );
        }
    }
}
#[test]
fn names_are_sorted_and_duplicate_name_tuples_are_removed() {
    assert_eq!(
        render(["z", "a", "z"]),
        include_str!("fixtures/tuple-output.txt")
    );
}

#[test]
fn colliding_ports_do_not_discard_distinct_service_names() {
    let values = include_str!("fixtures/collision.tsv")
        .trim_end()
        .split('\t')
        .collect::<Vec<_>>();
    assert_eq!(port(values[0]).to_string(), values[2]);
    assert_eq!(port(values[1]).to_string(), values[2]);
    let mut expected = [values[0], values[1]];
    expected.sort_unstable();
    assert_eq!(
        render([values[0], values[1]]),
        format!(
            "{} {}\n{} {}\n",
            expected[0], values[2], expected[1], values[2]
        )
    );
}
