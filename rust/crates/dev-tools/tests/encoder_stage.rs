use openpilot_dev_tools::{
    encoder_stage::{
        self, build_swscale, extract_native, verify_archive, verify_lock, verify_swscale_headers,
        NativeRunner, Package, Toolchain,
    },
    seal::digest,
};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};
fn package() -> Package {
    Package {
        name: "fixture".into(),
        version: "1.0".into(),
        commit: "source-pin".into(),
        sha256: "unused".into(),
        size: 1,
    }
}
fn wheel(root: &Path, entries: &[(&str, &[u8])]) -> PathBuf {
    let path = root.join("fixture.whl");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    path
}
fn lock(pin: &str) -> toml::Value {
    format!("[[package]]\nname='fixture'\nversion='1.0'\n[package.source]\ngit='repo#{pin}'\n")
        .parse()
        .unwrap()
}
#[test]
fn reviewed_pins_and_unique_locked_sources_are_required() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("archive");
    fs::write(&archive, b"owned package").unwrap();
    let mut p = package();
    assert!(verify_archive(&archive, &p)
        .unwrap_err()
        .to_string()
        .contains("SHA256"));
    p.sha256 = digest(&archive).unwrap();
    p.size = fs::metadata(&archive).unwrap().len();
    verify_archive(&archive, &p).unwrap();
    p.size += 1;
    assert!(verify_archive(&archive, &p).is_err());
    p.size -= 1;
    verify_lock(&lock("source-pin"), &p).unwrap();
    assert!(verify_lock(&lock("wrong"), &p)
        .unwrap_err()
        .to_string()
        .contains("locked source"));
    let mut duplicate = lock("source-pin");
    let first = duplicate["package"][0].clone();
    duplicate["package"].as_array_mut().unwrap().push(first);
    assert!(verify_lock(&duplicate, &p).is_err());
    let mut changed = lock("source-pin");
    changed["package"][0]["version"] = toml::Value::String("2.0".into());
    assert!(verify_lock(&changed, &p).is_err());
    let pins = encoder_stage::packages();
    assert_eq!(
        pins[0].sha256,
        "ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4"
    );
    assert_eq!(pins[0].size, 11743600);
    assert_eq!(
        pins[1].sha256,
        "1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475"
    );
    assert_eq!(pins[1].size, 267013);
    assert!(pins[0].url().ends_with(&pins[0].filename()));
}
#[test]
fn only_native_payload_is_extracted_with_streamed_hashes_and_exec_mode() {
    let temp = tempfile::tempdir().unwrap();
    let p = package();
    let prefix = p.prefix();
    let names = [
        "include/header.h",
        "lib/libfixture.a",
        "bin/ffmpeg",
        "bin/ffprobe",
    ]
    .map(|n| format!("{prefix}{n}"));
    let archive = wheel(
        temp.path(),
        &[
            (
                "fixture/__init__.py",
                b"raise RuntimeError('must never run')",
            ),
            (&names[0], b"header"),
            (&names[1], b"archive"),
            (&names[2], b"binary"),
            (&names[3], b"probe"),
        ],
    );
    let output = temp.path().join("native");
    let records = extract_native(&archive, &p, &output).unwrap();
    assert_eq!(records.len(), 4);
    assert_eq!(
        fs::read(output.join("include/header.h")).unwrap(),
        b"header"
    );
    assert!(!output.join("fixture").exists());
    for r in records {
        assert_eq!(r.sha256, digest(&output.join(&r.path)).unwrap());
        assert_eq!(r.bytes, fs::metadata(output.join(r.path)).unwrap().len());
    }
    for bin in ["ffmpeg", "ffprobe"] {
        assert_eq!(
            fs::metadata(output.join("bin").join(bin))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    assert!(extract_native(&archive, &p, &output).is_err());
}
fn patch_all(bytes: &mut [u8], old: &[u8], new: &[u8]) {
    assert_eq!(old.len(), new.len());
    let mut cursor = 0;
    while let Some(i) = bytes[cursor..].windows(old.len()).position(|v| v == old) {
        let i = cursor + i;
        bytes[i..i + old.len()].copy_from_slice(new);
        cursor = i + new.len();
    }
}
#[test]
fn invalid_zip_members_fail_before_creating_output() {
    for fault in [
        "traversal",
        "absolute",
        "symlink",
        "duplicate",
        "backslash",
        "empty",
        "dot",
        "file-directory",
        "oversized",
        "crc",
        "truncated",
        "missing",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let p = package();
        let prefix = p.prefix();
        let relative = match fault {
            "traversal" => "../outside",
            "absolute" => "/absolute",
            "backslash" => "lib\\outside",
            "empty" => "",
            "dot" => "lib/./outside",
            _ => "lib/one.a",
        };
        let name = format!("{prefix}{relative}");
        let other = format!("{prefix}lib/two.a");
        let nested = format!("{prefix}lib/one.a/child");
        let entries = match fault {
            "duplicate" => vec![
                (name.as_str(), b"owned".as_slice()),
                (other.as_str(), b"second".as_slice()),
            ],
            "file-directory" => vec![
                (name.as_str(), b"owned".as_slice()),
                (nested.as_str(), b"nested".as_slice()),
            ],
            "missing" => vec![("fixture/__init__.py", b"owned".as_slice())],
            _ => vec![(name.as_str(), b"owned".as_slice())],
        };
        let archive = wheel(temp.path(), &entries);
        let mut bytes = fs::read(&archive).unwrap();
        match fault {
            "duplicate" => patch_all(&mut bytes, other.as_bytes(), name.as_bytes()),
            "symlink" => {
                let at = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
                bytes[at + 38..at + 42].copy_from_slice(&(0o120777u32 << 16).to_le_bytes());
            }
            "oversized" => {
                let at = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
                bytes[at + 24..at + 28].copy_from_slice(&(256 * 1024 * 1024 + 1u32).to_le_bytes());
            }
            "crc" => patch_all(&mut bytes, b"owned", b"wrong"),
            "truncated" => {
                bytes.truncate(bytes.len() - 8);
            }
            _ => {}
        }
        fs::write(&archive, bytes).unwrap();
        let output = temp.path().join("native");
        assert!(
            extract_native(&archive, &p, &output).is_err(),
            "accepted {fault}"
        );
        assert!(!output.exists(), "partial output for {fault}");
        assert!(!temp.path().join("outside").exists());
    }
}
#[test]
fn wrong_scaler_source_never_reaches_configure() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.tar.xz");
    fs::write(&source, b"wrong source").unwrap();
    let output = temp.path().join("build");
    let error = build_swscale(
        &output,
        &temp.path().join("install"),
        Some(&source),
        &Toolchain {
            cc: "must-not-run".into(),
            ar: "must-not-run".into(),
            ranlib: "must-not-run".into(),
        },
        &mut NativeRunner,
    )
    .unwrap_err();
    assert!(error.to_string().contains("source SHA256"));
    assert!(!output.join("ffmpeg-7.1").exists());
    assert!(!output.join("build").exists());
}
#[test]
fn avutil_headers_require_byte_identity() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let install = temp.path().join("install");
    for name in ["version.h", "pixfmt.h"] {
        for base in [source.join("libavutil"), install.join("include/libavutil")] {
            fs::create_dir_all(&base).unwrap();
            fs::write(base.join(name), b"compatible source").unwrap();
        }
    }
    verify_swscale_headers(&source, &install).unwrap();
    fs::write(install.join("include/libavutil/pixfmt.h"), b"wrong ABI").unwrap();
    assert!(verify_swscale_headers(&source, &install)
        .unwrap_err()
        .to_string()
        .contains("avutil header"));
}
#[test]
fn cli_rejects_changed_lock_before_creating_staging() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("uv.lock"), "package=[]").unwrap();
    let output = temp.path().join("stage");
    let result = Command::new(env!("CARGO_BIN_EXE_stage-encoder-target"))
        .arg("--root")
        .arg(&root)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("locked source"));
    assert!(!output.exists());
}

#[test]
fn content_cache_cli_rejects_changed_source_lock_before_preparing_inputs() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("uv.lock"), "package=[]").unwrap();
    let cache = temp.path().join("assets");
    let store = temp.path().join("prefixes");
    let result = Command::new(env!("CARGO_BIN_EXE_stage-encoder-target"))
        .arg("--prepare-wheels")
        .arg("--root")
        .arg(temp.path())
        .arg("--cache")
        .arg(&cache)
        .arg("--store")
        .arg(&store)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("locked source"));
    assert!(!cache.exists() && !store.exists());
}
