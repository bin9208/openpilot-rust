use openpilot_manager_catalog::{
    binding::{self, Installation},
    catalog, ImportConfig,
};
use std::path::PathBuf;

fn entries() -> Vec<openpilot_manager_catalog::Descriptor> {
    catalog(ImportConfig {
        pc: false,
        tici: true,
        webcam: false,
        carrot_web_external: false,
        darwin: false,
        bodyteleop_available: true,
    })
}

#[test]
fn cluster_binding_carries_native_helpers_and_default_display_assets() {
    let installation = Installation {
        root: PathBuf::from("/owned/repo"),
        binaries: PathBuf::from("/owned/repo/rust/bin"),
    };
    let entry = entries()
        .into_iter()
        .find(|entry| entry.name == "carrot_cluster")
        .unwrap();
    let bound = binding::resolve(entry, &installation).unwrap();
    assert_eq!(
        bound.argv,
        [
            "/owned/repo/rust/bin/openpilot-cluster-autorun",
            "--root",
            "/owned/repo"
        ]
    );
    for name in ["openpilot-cluster", "ffmpeg", "ffprobe"] {
        assert!(
            bound.requirements.iter().any(|required| matches!(required,
            binding::Requirement::Executable(path) if path == &installation.binaries.join(name))),
            "missing helper: {name}"
        );
    }
    for name in [
        "rust/bin/cluster-replay/freesansbold.ttf",
        "openpilot/selfdrive/assets/fonts/KaiGenGothicKR-Bold.ttf",
        "openpilot/selfdrive/assets/icons_mici/carrot_cruse_gap_trimmed.png",
        "openpilot/selfdrive/assets/images/speed_bg.png",
        "openpilot/selfdrive/assets/images/traffic_red.png",
        "openpilot/selfdrive/assets/images/traffic_green.png",
        "openpilot/selfdrive/assets/icons_mici/carrot_wheel_org.png",
        "openpilot/selfdrive/assets/icons_mici/carrot_wheel_lane.png",
        "openpilot/selfdrive/assets/icons_mici/settings/network/wifi_strength_full.png",
        "openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png",
    ] {
        assert!(
            bound.requirements.iter().any(|required| matches!(required,
            binding::Requirement::File(path) if path == &installation.root.join(name))),
            "missing display asset: {name}"
        );
    }
}

#[test]
fn native_optional_endpoints_have_their_installed_providers_in_the_package_gate() {
    let installation = Installation {
        root: PathBuf::from("/owned/repo"),
        binaries: PathBuf::from("/owned/repo/rust/bin"),
    };
    for (name, directory) in [("carrot_man", "carrot-geos"), ("carrot_server", "brotli")] {
        let entry = entries()
            .into_iter()
            .find(|entry| entry.name == name)
            .unwrap();
        let bound = binding::resolve(entry, &installation).unwrap();
        assert!(bound.requirements.iter().any(|required| matches!(required,
            binding::Requirement::Directory(path) if path == &installation.binaries.join(directory))));
        assert_eq!(bound.argv.len(), 1);
    }
}

#[test]
fn encoder_binding_preserves_modes_when_one_binary_serves_variants() {
    let installation = Installation {
        root: PathBuf::from("/owned/repo"),
        binaries: PathBuf::from("/owned/repo/rust/bin"),
    };
    let entry = entries()
        .into_iter()
        .find(|entry| entry.name == "stream_encoderd")
        .unwrap();
    let bound = binding::resolve(entry, &installation).unwrap();
    assert_eq!(
        bound.argv,
        ["/owned/repo/rust/bin/openpilot-encoderd", "--stream"]
    );
}

#[test]
fn card_binding_requires_checkout_and_dbc_and_numerical_companions_when_selected() {
    let installation = Installation {
        root: PathBuf::from("/owned/repo"),
        binaries: PathBuf::from("/owned/repo/rust/bin"),
    };
    let entry = entries()
        .into_iter()
        .find(|entry| entry.name == "card")
        .unwrap();
    let bound = binding::resolve(entry, &installation).unwrap();
    assert_eq!(
        bound.argv,
        [
            "/owned/repo/rust/bin/openpilot-card",
            "--root",
            "/owned/repo",
            "--dbc",
            "/owned/repo/rust/bin/radarcan-dbc",
            "--numerics",
            "/owned/repo/rust/bin/card-numerics"
        ]
    );
    assert!(bound.requirements.iter().any(|requirement| matches!(requirement,
        binding::Requirement::Directory(path) if path == &installation.binaries.join("radarcan-dbc"))));
    assert!(bound.validate().is_err());
}

#[test]
fn persistent_binding_retains_pid_policy_when_athena_is_native() {
    let installation = Installation {
        root: PathBuf::from("/owned/repo"),
        binaries: PathBuf::from("/owned/repo/rust/bin"),
    };
    let entry = entries()
        .into_iter()
        .find(|entry| entry.name == "manage_athenad")
        .unwrap();
    let bound = binding::resolve(entry, &installation).unwrap();
    assert_eq!(bound.pid_key, Some("AthenadPid"));
    assert_eq!(
        bound.argv[0],
        "/owned/repo/rust/bin/openpilot-manage-athenad"
    );
}

#[test]
fn panda_binding_rejects_missing_empty_pointer_firmware_before_launch() {
    struct Owned(std::path::PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!(
        "panda-binding-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _owned = Owned(root.clone());
    let binaries = root.join("rust/bin");
    std::fs::create_dir_all(&binaries).unwrap();
    let firmware = root.join("panda/board/obj");
    std::fs::create_dir_all(&firmware).unwrap();
    for name in [
        "openpilot-pandad-supervisor",
        "openpilot-pandad",
        "openpilot-process-child",
    ] {
        std::fs::copy(std::env::current_exe().unwrap(), binaries.join(name)).unwrap();
    }
    let installation = Installation { root, binaries };
    let entry = entries()
        .into_iter()
        .find(|entry| entry.name == "pandad")
        .unwrap();
    let bound = binding::resolve(entry, &installation).unwrap();
    assert!(
        bound.validate().is_err(),
        "empty directory admitted firmware"
    );
    let names = [
        "panda.bin.signed",
        "panda_h7.bin.signed",
        "panda_tesla_wake.bin.signed",
        "panda_h7_tesla_wake.bin.signed",
        "bootstub.panda.bin",
        "bootstub.panda_h7.bin",
    ];
    for name in names {
        std::fs::write(firmware.join(name), [0x21; 256]).unwrap();
    }
    bound.validate().unwrap();
    for name in names {
        let path = firmware.join(name);
        std::fs::write(&path, []).unwrap();
        assert!(bound.validate().is_err(), "empty artifact admitted: {name}");
        std::fs::write(
            &path,
            b"version https://git-lfs.github.com/spec/v1\noid sha256:owned\nsize 256\n",
        )
        .unwrap();
        assert!(bound.validate().is_err(), "pointer admitted: {name}");
        std::fs::write(path, [0x21; 256]).unwrap();
    }
    bound.validate().unwrap();
}

#[test]
fn model_and_web_bindings_admit_required_source_assets_to_the_package() {
    let installation = Installation {
        root: PathBuf::from("/owned/repo"),
        binaries: PathBuf::from("/owned/repo/rust/bin"),
    };
    for (name, paths) in [
        (
            "xiaoge_data",
            [
                "openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx",
                "openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx",
            ],
        ),
        (
            "carrot_server",
            [
                "openpilot/selfdrive/carrot/web/assets/alert_camera.svg",
                "openpilot/selfdrive/carrot/web/assets/alert_police.svg",
            ],
        ),
    ] {
        let entry = entries()
            .into_iter()
            .find(|entry| entry.name == name)
            .unwrap();
        let bound = binding::resolve(entry, &installation).unwrap();
        for path in paths {
            assert!(
                bound.requirements.iter().any(|required| matches!(required,
                binding::Requirement::File(file) if file == &installation.root.join(path))),
                "source asset omitted from native package: {name}: {path}"
            );
        }
    }
}
