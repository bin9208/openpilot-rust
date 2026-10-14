use openpilot_dev_tools::{media_stage::stage, seal::digest};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    providers: PathBuf,
    ffmpeg: PathBuf,
    graphics: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("package");
        let providers = temp.path().join("providers");
        fs::create_dir_all(&root).unwrap();
        let binaries = [
            "sdl-validated/lib/libSDL2-2-6e5c51a6.0.so.0.2800.4",
            "sdl-validated/lib/libSDL2_ttf-2-e035a276.0.so.0.2000.1",
            "ffmpeg/install/lib/libavcodec.so.60.31.102",
            "media-sdk/usr/lib/aarch64-linux-gnu/libvpx.so.9.0.0",
            "raylib/lib/libopenpilot-raylib.so",
            "ffmpeg/install/bin/ffmpeg",
            "ffmpeg/install/bin/ffprobe",
        ];
        let mut objects = Vec::new();
        for name in binaries {
            let path = providers.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut bytes = vec![0; 32];
            bytes[..4].copy_from_slice(b"\x7fELF");
            bytes[4] = 2;
            bytes[5] = 1;
            bytes[18..20].copy_from_slice(&183_u16.to_le_bytes());
            fs::write(&path, bytes).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            objects.push(json!({"path":path,"sha256":digest(&path).unwrap()}));
        }
        symlink(
            "libavcodec.so.60.31.102",
            providers.join("ffmpeg/install/lib/libavcodec.so.60"),
        )
        .unwrap();
        for name in [
            "sdl-validated/freesansbold.ttf",
            "sdl-licenses/COPYING",
            "raylib/licenses/LICENSE",
            "ffmpeg/ffmpeg-6.1.1/COPYING.LGPLv2.1",
            "raylib/include/raylib.h",
            "raylib/plugin-build.json",
            "ffmpeg/build-record.json",
            "vpx-package-metadata.json",
            "vpx-sdk/usr/share/doc/libvpx9/copyright",
        ] {
            let path = providers.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, name).unwrap();
        }
        let ffmpeg = providers.join("ffmpeg-report.json");
        let graphics = providers.join("graphics-report.json");
        let report = json!({"passed":true,"objects":objects,"wheel_sha256":"a".repeat(64)});
        for path in [
            &ffmpeg,
            &graphics,
            &providers.join("sdl-validated/report.json"),
        ] {
            fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
        }
        Self {
            _temp: temp,
            root,
            providers,
            ffmpeg,
            graphics,
        }
    }
    fn run(&self) -> openpilot_dev_tools::seal::Result<Value> {
        stage(&self.root, &self.providers, &self.ffmpeg, &self.graphics)
    }
    fn edit_report(&self, change: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_slice(&fs::read(&self.ffmpeg).unwrap()).unwrap();
        change(&mut v);
        fs::write(&self.ffmpeg, serde_json::to_vec(&v).unwrap()).unwrap();
    }
}

#[test]
fn complete_native_providers_retain_bytes_modes_aliases_and_provenance() {
    let f = Fixture::new();
    let result = f.run().unwrap();
    assert_eq!(result["files"], 16);
    assert_eq!(result["aliases"], 4);
    let report: Value =
        serde_json::from_slice(&fs::read(f.root.join("rust/bin/media-provenance.json")).unwrap())
            .unwrap();
    for row in report["files"].as_array().unwrap() {
        let source = Path::new(row["source"].as_str().unwrap());
        let target = Path::new(row["path"].as_str().unwrap());
        assert_eq!(fs::read(source).unwrap(), fs::read(target).unwrap());
        assert_eq!(digest(target).unwrap(), row["sha256"]);
        assert_eq!(
            fs::metadata(source).unwrap().permissions().mode() & 0o777,
            fs::metadata(target).unwrap().permissions().mode() & 0o777
        );
    }
    for (name, target) in [
        ("libavcodec.so.60", "libavcodec.so.60.31.102"),
        ("libvpx.so.9", "libvpx.so.9.0.0"),
        ("libSDL2-2.0.so.0", "libSDL2-2-6e5c51a6.0.so.0.2800.4"),
        (
            "libSDL2_ttf-2.0.so.0",
            "libSDL2_ttf-2-e035a276.0.so.0.2000.1",
        ),
    ] {
        assert_eq!(
            fs::read_link(f.root.join("rust/bin/lib").join(name)).unwrap(),
            Path::new(target)
        );
    }
    assert_eq!(f.run().unwrap(), result);
}
#[test]
fn failed_missing_or_contradictory_evidence_cannot_stage_any_files() {
    for case in 0..4 {
        let f = Fixture::new();
        f.edit_report(|v| match case {
            0 => v["passed"] = json!(false),
            1 => v["passed"] = json!("true"),
            2 => v["objects"][0]["sha256"] = json!("b".repeat(64)),
            _ => {
                v["objects"] = json!([]);
            }
        });
        // Empty one report is valid only if another supplies all identities; remove them from every report too.
        if case == 3 {
            fs::copy(&f.ffmpeg, &f.graphics).unwrap();
            fs::copy(&f.ffmpeg, f.providers.join("sdl-validated/report.json")).unwrap();
        }
        assert!(f.run().is_err(), "case {case}");
        assert!(!f.root.join("rust").exists());
    }
}
#[test]
fn changed_native_bytes_or_wrong_architecture_fail_before_staging() {
    for wrong_arch in [false, true] {
        let f = Fixture::new();
        let p = f.providers.join("raylib/lib/libopenpilot-raylib.so");
        let mut bytes = fs::read(&p).unwrap();
        bytes[18] = if wrong_arch { 62 } else { 0 };
        fs::write(&p, bytes).unwrap();
        if wrong_arch {
            for report in [
                &f.ffmpeg,
                &f.graphics,
                &f.providers.join("sdl-validated/report.json"),
            ] {
                let mut v: Value = serde_json::from_slice(&fs::read(report).unwrap()).unwrap();
                for row in v["objects"].as_array_mut().unwrap() {
                    if row["path"] == json!(p) {
                        row["sha256"] = json!(digest(&p).unwrap());
                    }
                }
                fs::write(report, serde_json::to_vec(&v).unwrap()).unwrap();
            }
        }
        assert!(f.run().is_err());
        assert!(!f.root.join("rust").exists());
    }
}
#[test]
fn conflicting_files_aliases_and_directory_symlinks_preserve_package() {
    for case in 0..4 {
        let f = Fixture::new();
        let lib = f.root.join("rust/bin/lib");
        fs::create_dir_all(&lib).unwrap();
        let target = match case {
            0 => lib.join("libopenpilot-raylib.so"),
            1 => lib.join("libvpx.so.9"),
            2 => f.root.join("rust/bin/media-provenance.json"),
            _ => f.root.join("rust/bin/licenses"),
        };
        if case < 3 {
            fs::write(&target, b"preserve existing bytes").unwrap();
        } else {
            symlink(&f.providers, &target).unwrap();
        }
        assert!(f.run().is_err(), "case {case}");
        assert!(!lib.join("libavcodec.so.60.31.102").exists());
        if case < 3 {
            assert_eq!(fs::read(target).unwrap(), b"preserve existing bytes");
        } else {
            assert!(!f.providers.join("sdl").exists());
        }
    }
}
#[test]
fn provider_alias_cannot_escape_verified_tree() {
    let f = Fixture::new();
    let p = f.providers.join("ffmpeg/install/lib/libavcodec.so.60");
    fs::remove_file(&p).unwrap();
    symlink("../../../../outside.so", p).unwrap();
    assert!(f.run().is_err());
    assert!(!f.root.join("rust").exists());
}

#[test]
fn matching_bytes_with_nonexecutable_mode_are_not_a_valid_staged_tool() {
    let f = Fixture::new();
    let target = f.root.join("rust/bin/ffmpeg");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(f.providers.join("ffmpeg/install/bin/ffmpeg"), &target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        f.run().is_err(),
        "matching bytes cannot admit a nonexecutable codec tool"
    );
    assert!(!f.root.join("rust/bin/lib").exists());
    assert_eq!(
        fs::metadata(target).unwrap().permissions().mode() & 0o777,
        0o644
    );
}
