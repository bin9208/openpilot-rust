//! Real native startup CLI, with owned HTTP fixtures and no hardware/history gate.
use openpilot_usbgpu::{
    model::Manifest,
    model_delivery::{status, State},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};
fn command() -> Command {
    let binary = env!("CARGO_BIN_EXE_openpilot-usbgpu-model");
    if cfg!(target_arch = "aarch64") {
        if let Ok(runner) = std::env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_whitespace();
            let mut command = Command::new(words.next().unwrap());
            command.args(words).arg(binary);
            return command;
        }
    }
    Command::new(binary)
}
#[test]
fn startup_cli_returns_real_delivery_result_without_hardware_history_or_gpu() {
    for success in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        fs::create_dir(&cache).unwrap();
        let data = b"owned existing verified ONNX fixture";
        let model = Manifest {
            model_id: "startup-fixture".into(),
            filename: "fixture.onnx".into(),
            size: data.len() as u64,
            sha256: format!("{:x}", Sha256::digest(data)),
            url: "https://example.invalid/fixture.onnx".into(),
        };
        if success {
            fs::write(cache.join(model.cache_filename()), data).unwrap();
            fs::write(
                cache.join("state.json"),
                serde_json::to_vec(&json!({"active":model,"previous":null})).unwrap(),
            )
            .unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/manifest.json", listener.local_addr().unwrap());
        let body = serde_json::to_vec(&model).unwrap();
        let serve = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < until =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("startup CLI did not request delivery: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            if success {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            } else {
                stream
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .unwrap();
            }
        });
        let output = command()
            .args(["--prepare-for-startup", "--retry-network", "--root"])
            .arg(root.path())
            .arg("--assets")
            .arg(root.path().join("absent-assets"))
            .arg("--devices")
            .arg(root.path().join("absent-devices"))
            .args(["--manifest-url", &url])
            .env("CARROT_BIG_MODEL_DIR", &cache)
            .output()
            .unwrap();
        serve.join().unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let saved = status::read(&cache).unwrap();
        assert_eq!(
            saved.state,
            if success {
                status::Phase::Ready
            } else {
                status::Phase::Error
            }
        );
        if success {
            assert!(State::read(&cache).unwrap().active.is_some());
        } else {
            assert_eq!(saved.error_code.as_deref(), Some("download"));
            assert_eq!(saved.retry_count, Some(1));
            assert_eq!(saved.retry_in_seconds, Some(0));
        }
    }
}

#[test]
fn presence_cli_is_model_independent_and_uses_only_source_ids_and_speed() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let usb = root.path().join("usb/owned");
    fs::create_dir_all(&usb).unwrap();
    fs::write(usb.join("idVendor"), "add1\n").unwrap();
    fs::write(usb.join("idProduct"), "0001\n").unwrap();
    for (speed, present) in [
        ("480", false),
        ("4999", false),
        ("5000", true),
        ("10000", true),
    ] {
        fs::write(usb.join("speed"), speed).unwrap();
        let output = command()
            .args(["--present", "--devices"])
            .arg(root.path().join("usb"))
            .env(
                "CARROT_BIG_MODEL_MANIFEST",
                std::ffi::OsString::from_vec(vec![255]),
            )
            .env("CARROT_BIG_MODEL_DIR", root.path().join("no-model-cache"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            present,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!root.path().join("no-model-cache").exists());
    }
}

#[test]
fn startup_failure_suppresses_boot_and_selected_runtime_but_keeps_raw_cache() {
    use std::os::unix::ffi::OsStringExt;
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let model = openpilot_usbgpu::model_delivery::pinned_manifest();
    let file = fs::File::create(cache.join(model.cache_filename())).unwrap();
    file.set_len(model.size).unwrap();
    fs::write(
        cache.join("state.json"),
        serde_json::to_vec(&json!({"active":model,"previous":null})).unwrap(),
    )
    .unwrap();
    let args = |action: &str, failed: bool| {
        let mut cmd = command();
        cmd.arg(action)
            .arg("--root")
            .arg(root.path())
            .arg("--assets")
            .arg(root.path().join("missing-assets"))
            .env("CARROT_BIG_MODEL_DIR", &cache)
            .env_remove("CARROT_BIG_MODEL_MANIFEST");
        if failed {
            cmd.env("CARROT_BIG_MODEL_STARTUP_FAILED", "1");
        } else {
            cmd.env_remove("CARROT_BIG_MODEL_STARTUP_FAILED");
        }
        cmd
    };
    let selected = args("--active-sha", false).output().unwrap();
    assert!(selected.status.success());
    assert_eq!(
        String::from_utf8(selected.stdout).unwrap().trim(),
        model.sha256
    );
    for action in ["--active-sha", "--active-path"] {
        let output = args(action, true).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"\n");
    }
    let output = args("--boot", true)
        .env(
            "CARROT_BIG_MODEL_MANIFEST",
            std::ffi::OsString::from_vec(vec![255]),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        json!({"prepared":false})
    );
    assert!(!args("--ready", true).output().unwrap().status.success());
    assert_eq!(
        State::read(&cache).unwrap().active.unwrap().sha256,
        model.sha256
    );
    let state = fs::read_to_string(cache.join("state.json")).unwrap();
    assert!(!state.contains("manifest_url"));
}

#[test]
fn verified_custom_catalog_binding_refreshes_metadata_without_changing_bytes() {
    let root = tempfile::tempdir().unwrap();
    let mut model = openpilot_usbgpu::model_delivery::pinned_manifest();
    fs::write(
        root.path().join("state.json"),
        serde_json::to_vec(&json!({"active":model,"previous":null,"manifest_url":"old"})).unwrap(),
    )
    .unwrap();
    model.model_id = "updated-label".into();
    openpilot_usbgpu::model_delivery::bind_manifest(
        root.path(),
        &model,
        "https://custom.invalid/manifest.json",
    )
    .unwrap();
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("state.json")).unwrap()).unwrap();
    assert_eq!(
        value["manifest_url"],
        "https://custom.invalid/manifest.json"
    );
    assert_eq!(value["active"]["model_id"], "updated-label");
}
