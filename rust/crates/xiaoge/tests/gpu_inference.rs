#![cfg(feature = "native-skip-miri")]
use openpilot_opencv_runtime as cv;
use openpilot_xiaoge::{
    compute::ComputeMode,
    inference::LaneModel,
    nv12::{Frame, Layout},
};
use std::path::Path;

#[test]
fn cpu_only_runner_reports_gpu_fallback_without_changing_detections_or_repeating_loads() {
    if std::env::var("OPENCV_TEST_GPU").as_deref() != Ok("reject") {
        return;
    }
    cv::initialize(2).unwrap();
    let path = std::env::var("OPENCV_TEST_LANE_MODEL").expect("pinned lane model path");
    let mut model = LaneModel::load(Path::new(&path));
    assert!(model.loaded(), "{}", model.error());
    let bytes = vec![128; 640 * 480 * 3 / 2];
    let frame = Frame::new(
        &bytes,
        Layout {
            width: 640,
            height: 480,
            stride: 640,
            uv_offset: 640 * 480,
        },
    )
    .unwrap();
    let baseline = model.infer(&frame, 0.25);
    assert!(baseline.valid, "{}", baseline.error);
    assert_eq!(model.compute_status()["effective_backend"], "cpu");
    model.set_compute_mode(ComputeMode::Gpu, 1);
    let status = model.compute_status();
    assert_eq!(status["mode"], 1);
    assert_eq!(status["effective_backend"], "cpu");
    assert!(!status["fallback_reason"].as_str().unwrap().is_empty());
    for _ in 0..2 {
        let result = model.infer(&frame, 0.25);
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::to_value(&baseline).unwrap()
        );
        assert_eq!(
            model.compute_status()["initialization_ms"],
            status["initialization_ms"]
        );
        assert!(model.compute_timing()["comparison"].is_null());
        assert_eq!(model.compute_timing()["compare_wall_ms"], 0.0);
    }
    model.set_compute_mode(ComputeMode::Compare, 2);
    let result = model.infer(&frame, 0.25);
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::to_value(baseline).unwrap()
    );
    assert_eq!(model.compute_status()["mode"], 2);
    assert_eq!(model.compute_status()["effective_backend"], "cpu");
    assert!(
        model.compute_timing()["comparison"].is_null(),
        "no false CPU/GPU comparison without a GPU"
    );
    model.set_compute_mode(ComputeMode::Cpu, 3);
    let status = model.compute_status();
    assert_eq!(status["mode"], 0);
    assert_eq!(status["effective_backend"], "cpu");
    assert_eq!(status["fallback_reason"], "");
}

#[test]
fn diagnostic_settings_persist_mode_reject_invalid_changes_and_return_to_cpu() {
    use openpilot_logmessaged::JsonValue;
    use openpilot_xiaoge::{
        config::Config,
        native::shared::Shared,
        service::{Model, State},
    };
    use std::sync::{atomic::AtomicBool, Condvar, Mutex};
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!("xiaoge-settings-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = Cleanup(root);
    let params = openpilot_params::Params::open(&root.0, "settings").unwrap();
    let shared = Shared {
        state: Mutex::new(State::new(
            Config::default(),
            ["bsd", "lane"].map(|path| Model {
                path: path.into(),
                loaded: false,
                error: String::new(),
            }),
            1.0,
        )),
        snapshot: Condvar::new(),
        vasm_operation: Mutex::new(()),
        params: Some(params.clone()),
        params_operation: Mutex::new(()),
        config_path: root.0.join("config.json"),
        running: AtomicBool::new(true),
    };
    shared
        .set_settings(&JsonValue::parse("{\"computeMode\":1}").unwrap())
        .unwrap();
    assert_eq!(params.get("OnnxComputeMode").unwrap(), Some(b"1".to_vec()));
    shared.refresh(true).unwrap();
    assert_eq!(shared.state().unwrap().compute_mode, ComputeMode::Gpu);
    let status: serde_json::Value = serde_json::from_slice(&shared.status().unwrap()).unwrap();
    assert_eq!(status["computeMode"], 1);
    for invalid in ["3", "true", "1.5", "\"1\""] {
        let request =
            JsonValue::parse(&format!("{{\"computeMode\":{invalid},\"threshold\":0.5}}")).unwrap();
        assert!(shared.set_settings(&request).is_err());
        assert_eq!(params.get("OnnxComputeMode").unwrap(), Some(b"1".to_vec()));
        assert_eq!(shared.state().unwrap().settings.threshold, 0.94);
    }
    shared
        .set_settings(&JsonValue::parse("{\"computeMode\":0}").unwrap())
        .unwrap();
    shared.refresh(true).unwrap();
    assert_eq!(params.get("OnnxComputeMode").unwrap(), Some(b"0".to_vec()));
    assert_eq!(shared.state().unwrap().compute_mode, ComputeMode::Cpu);
    let status: serde_json::Value = serde_json::from_slice(&shared.status().unwrap()).unwrap();
    assert!(status.get("compute").is_none());
}
