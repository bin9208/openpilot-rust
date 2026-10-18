//! Actual raw process protocol, mechanical input polling and owned cleanup.
#![cfg(feature = "image-array-native")]
use openpilot_dev_tools::{
    ui_application_lifecycle_raw::{build, Raw},
    ui_forms_input::Canvas,
};
use std::{fs, path::Path};

#[test]
fn raw_bad_handshake_is_an_error_and_its_owned_child_is_reaped() {
    // cat returns the exact library argument error and exits; it cannot emulate
    // the raw handshake, and no interpreter/helper script is involved.
    assert!(Raw::open(Path::new("/bin/cat"), Path::new("/nonexistent/raylib")).is_err());
    assert!(Raw::open(
        Path::new("/nonexistent/application-raw"),
        Path::new("unused")
    )
    .is_err());
}

#[test]
fn failed_application_window_rejects_font_work_and_quits_cleanly_when_available() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let Some(root) = std::env::var_os("STARTUP_UI_RAYLIB_ROOT") else {
        return;
    };
    let library =
        std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY").expect("actual raylib provider required");
    assert!(!Path::new("/tmp/.X11-unix/X65534").exists());
    let temp = tempfile::tempdir().unwrap();
    let binary = build(temp.path(), Path::new(&root)).unwrap();
    let mut child = Command::new(binary)
        .arg(library)
        .env("DISPLAY", ":65534")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"app-window 64 64 7161\napp-font 0 plain 2f6465762f6e756c6c 10 0\nquit\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        format!(
            "OK\nERR {}\nERR {}\nOK\n",
            openpilot_dev_tools::ui_forms_input::hex("application window initialization failed"),
            openpilot_dev_tools::ui_forms_input::hex("forms window missing"),
        )
    );
}

#[test]
fn actual_independent_raw_input_clock_frame_export_and_cleanup_when_available() {
    let Some(root) = std::env::var_os("STARTUP_UI_RAYLIB_ROOT") else {
        return;
    };
    let Some(library) = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY") else {
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let binary = build(temp.path(), Path::new(&root)).unwrap();
    let output = temp.path().join("actual.png");
    let mut raw = Raw::open(&binary, Path::new(&library)).unwrap();
    raw.call("flags 128").unwrap();
    raw.call("window 536 240 496e646570656e64656e74206170706c69636174696f6e")
        .unwrap();
    raw.call("fps-limit 0").unwrap();
    assert_eq!(raw.request("window-close").unwrap(), "0");
    raw.call("poll-input").unwrap();
    let time = raw.request("clock").unwrap().parse::<f64>().unwrap();
    assert!(time >= 0.0);
    let mouse = raw
        .request("mouse-input")
        .unwrap()
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(mouse.len(), 6);
    let keyboard = raw
        .request("keyboard-input")
        .unwrap()
        .split_whitespace()
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(keyboard.len(), 6);
    raw.call("begin").unwrap();
    raw.call("clear 4278190080").unwrap();
    raw.call(&format!(
        "export {}",
        openpilot_dev_tools::ui_forms_input::hex(output.to_str().unwrap())
    ))
    .unwrap();
    raw.call("end").unwrap();
    raw.close().unwrap();
    assert!(fs::metadata(&output).unwrap().len() > 0);
    let image = openpilot_dev_tools::image_array::read(&output).unwrap();
    assert_eq!(image.shape, [240, 536, 4]);
}

#[test]
fn actual_owned_window_receives_xtest_keys_motion_clicks_and_reports_protocol_errors() {
    let Some(root) = std::env::var_os("STARTUP_UI_RAYLIB_ROOT") else {
        return;
    };
    let library = std::env::var_os("STARTUP_UI_RAYLIB_LIBRARY").expect("actual raylib library");
    let display = std::env::var("DISPLAY").expect("owned private display");
    let temp = tempfile::tempdir().unwrap();
    let binary = build(temp.path(), Path::new(&root)).unwrap();
    let mut raw = Raw::open(&binary, Path::new(&library)).unwrap();
    let hex = openpilot_dev_tools::ui_forms_input::hex;
    let title = "Independent application XTest peer";
    raw.call(&format!("app-window 536 240 {}", hex(title)))
        .unwrap();
    raw.call(&format!("x11-open {}", hex(&display))).unwrap();
    let window = raw
        .request(&format!("x11-window {}", hex(title)))
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert_ne!(window, 0);
    raw.call(&format!("x11-focus {window}")).unwrap();
    raw.call("x11-sync").unwrap();
    raw.call("poll-input").unwrap();
    raw.call(&format!("x11-key {} 1", hex("n"))).unwrap();
    raw.call("x11-sync").unwrap();
    // XSync completes the injecting connection; the window's separate Xlib
    // connection still has to receive and dispatch those events.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let keys = loop {
        raw.call("poll-input").unwrap();
        let keys = raw
            .request("keyboard-input")
            .unwrap()
            .split_whitespace()
            .map(str::parse::<i64>)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        if keys[1] != 0 || std::time::Instant::now() >= deadline {
            break keys;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(keys[1], i64::from(b'n'));
    raw.call(&format!("x11-key {} 0", hex("n"))).unwrap();
    raw.call(&format!("x11-motion {window} 110 90")).unwrap();
    raw.call("x11-button 1").unwrap();
    raw.call("x11-sync").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let mouse = loop {
        raw.call("poll-input").unwrap();
        let mouse = raw
            .request("mouse-input")
            .unwrap()
            .split_whitespace()
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        if mouse[0] != 0. || std::time::Instant::now() >= deadline {
            break mouse;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert_eq!(&mouse[..3], &[1., 110., 90.]);
    raw.call("x11-button 0").unwrap();
    raw.call("x11-sync").unwrap();
    assert_eq!(raw.request("x11-errors").unwrap(), "0");
    // A bad foreign window exercises the actual Xlib error handler; no real
    // window is changed by this invalid identifier.
    raw.call("x11-focus 4294967295").unwrap();
    raw.call("x11-sync").unwrap();
    assert_ne!(raw.request("x11-errors").unwrap(), "0");
    raw.call("x11-close").unwrap();
    raw.close().unwrap();
}
