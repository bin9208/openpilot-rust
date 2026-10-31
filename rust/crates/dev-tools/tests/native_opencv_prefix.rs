use std::{path::Path, process::Command};

#[test]
fn opencv_prefix_rejects_an_independently_valid_foreign_profile_before_preparation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["native-ffmpeg-provider.json", "native-raylib-provider.json"] {
        let temp = tempfile::tempdir().unwrap();
        let store = temp.path().join("store");
        let result = Command::new(env!("CARGO_BIN_EXE_build-runtime-opencv"))
            .arg("--prepare-released-prefix")
            .arg("--header-plan")
            .arg(root.join("native-opencv-headers.json"))
            .arg("--provider-plan")
            .arg(root.join(name))
            .arg("--descriptor")
            .arg(temp.path().join("missing"))
            .arg("--descriptor-sha256")
            .arg("0".repeat(64))
            .arg("--header-cache")
            .arg(temp.path().join("headers"))
            .arg("--provider-cache")
            .arg(temp.path().join("providers"))
            .arg("--store")
            .arg(&store)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr)
            .contains("OpenCV prefix requires its original OpenCV"));
        assert!(!store.exists());
    }
}
