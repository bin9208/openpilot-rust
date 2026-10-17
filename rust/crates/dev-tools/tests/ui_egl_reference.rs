use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command};
fn compile(source: &Path, output: &Path) {
    openpilot_disk_space::inspect(output, 64 * 1024 * 1024)
        .unwrap()
        .require()
        .unwrap();
    let result = Command::new("g++")
        .args(["-shared", "-fPIC", "-std=c++17"])
        .arg(source)
        .args(["-ldl", "-o"])
        .arg(output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
}
fn run(library: &Path, case: &str, trace: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ui-egl-reference"))
        .env("LD_LIBRARY_PATH", library)
        .env("EGL_TRACE", trace)
        .env("EGL_CASE", case)
        .output()
        .unwrap()
}
#[test]
fn independent_original_reference_states_and_traces_across_actual_abi_variants() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-egl");
    let baseline: Value =
        serde_json::from_str(include_str!("fixtures/ui-egl/original-reference.json")).unwrap();
    let closed: Value =
        serde_json::from_str(include_str!("fixtures/ui-egl/original-closed-fd.json")).unwrap();
    let variants: Value =
        serde_json::from_str(include_str!("fixtures/ui-egl/original-variants.json")).unwrap();
    for variant in [
        "baseline",
        "alternate-gles",
        "partial-load-fd",
        "missing-gles-symbol",
        "missing-destroy",
        "retry-init-failure",
        "destroy-closes-fd",
        "rejected-create-closes-fd",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path();
        let egl = if variant == "baseline" {
            root.join("rust/tools/ui_qa/egl_fixture.cc")
        } else {
            fixtures.join(variant).join("egl.cc")
        };
        let expected_hash = if variant == "baseline" {
            baseline["sources"]["rust/tools/ui_qa/egl_fixture.cc"]
                .as_str()
                .unwrap()
        } else {
            let rows = if variant.ends_with("closes-fd") {
                &closed
            } else {
                &variants
            };
            let row = rows
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["variant"] == variant)
                .unwrap();
            row[if variant.ends_with("closes-fd") {
                "source_sha256"
            } else {
                "egl_source_sha256"
            }]
            .as_str()
            .unwrap()
        };
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&egl).unwrap())),
            expected_hash,
            "{variant} original fixture source provenance"
        );
        compile(&egl, &directory.join("libEGL.so"));
        if variant == "partial-load-fd" {
            fs::write(directory.join("libGLESv2.so"), b"preexisting").unwrap();
        } else if fixtures.join(variant).join("gles.cc").exists() {
            let row = variants
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["variant"] == variant)
                .unwrap();
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(fixtures.join(variant).join("gles.cc")).unwrap())
                ),
                row["gles_source_sha256"].as_str().unwrap(),
                "{variant} GLES source provenance"
            );
            compile(
                &fixtures.join(variant).join("gles.cc"),
                &directory.join("libGLESv2.so"),
            );
        } else {
            std::os::unix::fs::symlink("libEGL.so", directory.join("libGLESv2.so")).unwrap();
        }
        let rows = if variant == "baseline" {
            baseline["cases"]
                .as_array()
                .unwrap()
                .iter()
                .collect::<Vec<_>>()
        } else {
            (if variant.ends_with("closes-fd") {
                &closed
            } else {
                &variants
            })
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["variant"] == variant)
            .collect()
        };
        for row in rows {
            let case = row["case"].as_str().unwrap();
            let trace = directory.join("trace.jsonl");
            fs::write(&trace, b"").unwrap();
            let result = run(directory, case, &trace);
            if row["exit"] == 1 {
                assert_eq!(
                    result.status.code(),
                    Some(1),
                    "{variant}/{case}: {result:?}"
                );
                assert!(result.stdout.is_empty());
                assert!(!result.stderr.is_empty());
            } else {
                assert!(result.status.success(), "{variant}/{case}: {result:?}");
                let state: Value = serde_json::from_slice(&result.stdout).unwrap();
                assert_eq!(state, row["state"], "{variant}/{case}");
            }
            let actual = fs::read_to_string(&trace).unwrap();
            let expected = if variant == "baseline" {
                row["trace"].as_array().unwrap().clone()
            } else {
                let hex = row["trace_hex"].as_str().unwrap();
                let bytes = hex
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                    .collect::<Vec<_>>();
                String::from_utf8(bytes)
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect()
            };
            let actual = actual
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{variant}/{case}");
            if row["exit"] != 1 {
                assert!(result.stderr.is_empty(), "{:?}", result.stderr);
            }
        }
    }
}
#[test]
fn raw_abi_rejects_unloaded_calls_and_unknown_handles() {
    use openpilot_startup_ui::egl_reference_abi::{Core, Extension, RawApi};
    let mut api = RawApi::new();
    assert!(api.display().is_err());
    assert!(api.initialize(1).is_err());
    assert!(api.extensions(1).is_err());
    assert!(api.error().is_err());
    assert!(api.active(0x84c0).is_err());
    assert!(api.bind(0x8d65, 17).is_err());
    assert!(api.image_target(0x8d65, 1).is_err());
    assert!(api.create(1, 0x3270, &[]).is_err());
    assert!(api.destroy(1, 1).is_err());
    assert!(api.resolve_core(Core::Current).is_err());
    assert!(api.resolve_proc(Extension::Create).is_err());
    assert!(api.resolve_library(Extension::Create).is_err());
}
#[test]
fn live_raw_abi_validates_attributes_and_display_image_token_ownership() {
    const MARKER: &str = "OPENPILOT_EGL_ABI_VALIDATION_CHILD";
    if std::env::var_os(MARKER).is_some() {
        use openpilot_startup_ui::egl_reference_abi::{Core, Extension, RawApi};
        use std::os::fd::AsRawFd;
        let mut api = RawApi::new();
        api.load_egl().unwrap();
        api.load_gles().unwrap();
        for symbol in [
            Core::Current,
            Core::Initialize,
            Core::Query,
            Core::Proc,
            Core::Error,
            Core::Bind,
            Core::Active,
        ] {
            api.resolve_core(symbol).unwrap();
        }
        let display = api.display().unwrap();
        assert_ne!(display, 0);
        assert!(api.initialize(display).unwrap());
        for symbol in [
            Extension::Create,
            Extension::Destroy,
            Extension::ImageTarget,
        ] {
            if !api.resolve_proc(symbol).unwrap() {
                api.resolve_library(symbol).unwrap();
            }
        }
        assert!(api.initialize(u64::MAX).is_err());
        assert!(api.extensions(u64::MAX).is_err());
        assert!(api.create(display, 0x3270, &[]).is_err());
        let file = fs::File::open("/dev/null").unwrap();
        let fd = file.as_raw_fd();
        let mut attrs = [
            0x3057, 1928, 0x3056, 1208, 0x3271, 842094158, 0x3272, fd, 0x3273, 0, 0x3274, 2048,
            0x3275, fd, 0x3276, 2473984, 0x3277, 2048, 0,
        ];
        assert!(api.create(display, 0x3270, &attrs).is_err());
        attrs[18] = 0x3038;
        assert!(api.create(u64::MAX, 0x3270, &attrs).is_err());
        let image = api.create(display, 0x3270, &attrs).unwrap();
        assert_ne!(image, 0);
        assert!(api.image_target(0x8d65, u64::MAX).is_err());
        assert!(api.destroy(display, u64::MAX).is_err());
        assert!(api.destroy(u64::MAX, image).is_err());
        assert!(api.destroy(display, image).unwrap());
        assert!(api.destroy(display, image).is_err());
        assert!(api.image_target(0x8d65, image).is_err());
        assert!(api.load_egl().is_err());
        assert!(api.load_gles().is_err());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    compile(
        &root.join("rust/tools/ui_qa/egl_fixture.cc"),
        &temp.path().join("libEGL.so"),
    );
    std::os::unix::fs::symlink("libEGL.so", temp.path().join("libGLESv2.so")).unwrap();
    let trace = temp.path().join("trace.jsonl");
    fs::write(&trace, b"").unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_raw_abi_validates_attributes_and_display_image_token_ownership",
            "--nocapture",
        ])
        .env(MARKER, "1")
        .env("LD_LIBRARY_PATH", temp.path())
        .env("EGL_CASE", "success")
        .env("EGL_TRACE", trace)
        .output()
        .unwrap();
    assert!(result.status.success(), "{result:?}");
}
