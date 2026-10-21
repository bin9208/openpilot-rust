#![cfg(feature = "native-skip-miri")]

use openpilot_opencv_runtime::{self as cv, DnnNet, DnnTarget, TensorView};
use std::{
    env, fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = env::temp_dir().join(format!(
            "opencv-target-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn model(&self) -> PathBuf {
        let path = self.0.join("conv-relu.onnx");
        fs::write(&path, tiny_model()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// Minimal synthetic ONNX: y = Relu(Conv(x, weight=2, bias=-1)).
// No external model, Python, protobuf tool or generated fixture is required.
fn varint(mut n: u64) -> Vec<u8> {
    let mut v = Vec::new();
    while n >= 128 {
        v.push(n as u8 | 128);
        n >>= 7;
    }
    v.push(n as u8);
    v
}
fn int(field: u64, value: u64) -> Vec<u8> {
    [varint(field << 3), varint(value)].concat()
}
fn bytes(field: u64, value: &[u8]) -> Vec<u8> {
    [
        varint(field << 3 | 2),
        varint(value.len() as u64),
        value.to_vec(),
    ]
    .concat()
}
fn value_info(name: &str) -> Vec<u8> {
    let shape: Vec<u8> = [1, 1, 2, 2]
        .into_iter()
        .flat_map(|d| bytes(1, &int(1, d)))
        .collect();
    let tensor = [int(1, 1), bytes(2, &shape)].concat();
    [bytes(1, name.as_bytes()), bytes(2, &bytes(1, &tensor))].concat()
}
fn initializer(name: &str, shape: &[u64], value: f32) -> Vec<u8> {
    [
        shape.iter().flat_map(|&d| int(1, d)).collect(),
        int(2, 1),
        bytes(8, name.as_bytes()),
        bytes(9, &value.to_le_bytes()),
    ]
    .concat()
}
fn tiny_model() -> Vec<u8> {
    let kernel = [bytes(1, b"kernel_shape"), int(8, 1), int(8, 1), int(20, 7)].concat();
    let conv = [
        bytes(1, b"x"),
        bytes(1, b"w"),
        bytes(1, b"b"),
        bytes(2, b"z"),
        bytes(3, b"conv"),
        bytes(4, b"Conv"),
        bytes(5, &kernel),
    ]
    .concat();
    let relu = [
        bytes(1, b"z"),
        bytes(2, b"y"),
        bytes(3, b"relu"),
        bytes(4, b"Relu"),
    ]
    .concat();
    let graph = [
        bytes(1, &conv),
        bytes(1, &relu),
        bytes(2, b"adapter-test"),
        bytes(5, &initializer("w", &[1, 1, 1, 1], 2.)),
        bytes(5, &initializer("b", &[1], -1.)),
        bytes(11, &value_info("x")),
        bytes(12, &value_info("y")),
    ]
    .concat();
    [int(1, 7), bytes(7, &graph), bytes(8, &int(2, 11))].concat()
}
fn infer(net: &mut DnnNet, values: &[f32]) -> Vec<cv::Tensor> {
    let names = net.output_names().unwrap();
    net.forward(TensorView::new(&[1, 1, 2, 2], values).unwrap(), &names)
        .unwrap()
}

// Catches load() accidentally opting in, reports inferred before execution,
// and losing copied output ownership after the next call/net destruction.
#[test]
fn cpu_default_and_explicit_cpu_preserve_math_and_owned_outputs() {
    cv::initialize(2).unwrap();
    let fixture = Fixture::new();
    let path = fixture.model();
    let mut net = DnnNet::load(&path).unwrap();
    let before = net.report().unwrap();
    assert_eq!(before.requested_target, DnnTarget::Cpu);
    assert_eq!(before.effective_target, None);
    assert_eq!(before.graph_gpu_layers, None);
    assert_eq!(before.priority_status, "not_applicable_cpu");
    let first = infer(&mut net, &[-1., 1., 2., 3.]);
    assert_eq!(first[0].view().values(), &[0., 1., 3., 5.]);
    let after = net.report().unwrap();
    assert_eq!(after.effective_target, Some(DnnTarget::Cpu));
    assert_eq!(after.graph_gpu_layers, Some(0));
    assert!(after.graph_cpu_layers.unwrap() > 0);
    assert_eq!(after.execution_status, "cpu");
    let _ = infer(&mut net, &[0.; 4]);
    drop(net);
    assert_eq!(first[0].view().values(), &[0., 1., 3., 5.]);
    let mut explicit = DnnNet::load_with_target(&path, DnnTarget::Cpu).unwrap();
    assert_eq!(
        infer(&mut explicit, &[-1., 1., 2., 3.])[0].view().values(),
        &[0., 1., 3., 5.]
    );
}

// A failed forward must not leave a previous success advertised as current.
#[test]
fn failed_forward_invalidates_execution_evidence() {
    cv::initialize(2).unwrap();
    let fixture = Fixture::new();
    let mut net = DnnNet::load(&fixture.model()).unwrap();
    let _ = infer(&mut net, &[1.; 4]);
    assert!(net
        .forward(
            TensorView::new(&[1, 1, 2, 2], &[1.; 4]).unwrap(),
            &["missing".into()]
        )
        .is_err());
    let report = net.report().unwrap();
    assert_eq!(report.effective_target, None);
    assert_eq!(report.graph_cpu_layers, None);
    assert_eq!(report.execution_status, "forward_failed");
    assert_eq!(
        infer(&mut net, &[-1., 1., 2., 3.])[0].view().values(),
        &[0., 1., 3., 5.]
    );
    let recovered = net.report().unwrap();
    assert_eq!(recovered.effective_target, Some(DnnTarget::Cpu));
    assert!(recovered.graph_cpu_layers.unwrap() > 0);
    assert_eq!(net.report().unwrap(), recovered);
    assert!(net
        .forward(
            TensorView::new(&[1, 1, 2, 2], &[1.; 4]).unwrap(),
            &["y\0bad".into()]
        )
        .is_err());
    assert_eq!(net.report().unwrap().graph_cpu_layers, None);
    assert_eq!(
        infer(&mut net, &[-1., 1., 2., 3.])[0].view().values(),
        &[0., 1., 3., 5.]
    );
}

// All environment changes stay in fresh child processes. The probe logs library
// loading itself, so merely initializing OpenCL (even without platforms) fails CPU.
#[test]
fn cpu_never_loads_opencl_and_gpu_rejects_a_runtime_without_devices() {
    if env::var_os("OPENCV_POLICY_CHILD").is_some() {
        return;
    }
    let fixture = Fixture::new();
    let probe = fixture.0.join("libOpenCL-probe.so");
    let compiler = env::var_os("CC").unwrap_or_else(|| "cc".into());
    assert!(Command::new(compiler)
        .args(["-shared", "-fPIC", "-o"])
        .arg(&probe)
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opencl_probe.c"))
        .status()
        .unwrap()
        .success());
    for mode in ["cpu", "gpu"] {
        let marker = fixture.0.join(mode);
        let output = Command::new(env::current_exe().unwrap())
            .args(["--exact", "policy_child", "--nocapture"])
            .env("OPENCV_POLICY_CHILD", mode)
            .env("OPENCV_OPENCL_RUNTIME", &probe)
            .env("OPENCV_POLICY_MARKER", &marker)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if mode == "cpu" {
            assert!(!marker.exists(), "CPU initialized the OpenCL loader");
        }
    }
}
#[test]
fn policy_child() {
    let Ok(mode) = env::var("OPENCV_POLICY_CHILD") else {
        return;
    };
    cv::initialize(2).unwrap();
    let fixture = Fixture::new();
    let path = fixture.model();
    if mode == "cpu" {
        let mut net = DnnNet::load(&path).unwrap();
        assert_eq!(
            infer(&mut net, &[-1., 1., 2., 3.])[0].view().values(),
            &[0., 1., 3., 5.]
        );
        assert_eq!(net.report().unwrap().effective_target, Some(DnnTarget::Cpu));
    } else {
        assert!(
            DnnNet::load_with_target(&path, DnnTarget::OpenClFp32).is_err(),
            "unsupported GPU must not silently become CPU"
        );
        let mut cpu = DnnNet::load(&path).unwrap();
        assert_eq!(
            infer(&mut cpu, &[-1., 1., 2., 3.])[0].view().values(),
            &[0., 1., 3., 5.]
        );
    }
}

// Explicit CI matrix gate: `reject` on an OpenCL-disabled provider and on a
// CPU-only ICD (e.g. PoCL); `require` only on a runner with an actual GPU.
#[test]
fn gpu_candidate_reports_graph_truth_and_cpu_gpu_cpu_lifetimes() {
    let Ok(expect) = env::var("OPENCV_TEST_GPU") else {
        return;
    };
    assert!(expect == "reject" || expect == "require");
    cv::initialize(2).unwrap();
    let fixture = Fixture::new();
    let path = fixture.model();
    let loaded = DnnNet::load_with_target(&path, DnnTarget::OpenClFp32);
    if expect == "reject" {
        assert!(loaded.is_err(), "CPU-only/disabled OpenCL accepted as GPU");
        return;
    }
    let mut gpu = loaded.unwrap();
    let before = gpu.report().unwrap();
    assert_eq!(before.effective_target, None);
    assert!(!before.device.is_empty());
    assert!(!before.vendor.is_empty());
    assert!(!before.driver.is_empty());
    assert!(!before.priority_status.is_empty());
    let first = infer(&mut gpu, &[-1., 1., 2., 3.]);
    for (&actual, expected) in first[0].view().values().iter().zip([0., 1., 3., 5.]) {
        assert!((actual - expected).abs() <= 1e-5);
    }
    let report = gpu.report().unwrap();
    eprintln!("GPU candidate: {report:?}");
    assert_eq!(report.effective_target, Some(DnnTarget::OpenClFp32));
    assert!(report.graph_gpu_layers.unwrap() > 0);
    assert_eq!(
        report.execution_status,
        "opencl_active_kernel_fallback_unknown"
    );
    let mut cpu = DnnNet::load(&path).unwrap();
    assert_eq!(
        infer(&mut cpu, &[-1., 1., 2., 3.])[0].view().values(),
        &[0., 1., 3., 5.]
    );
    let _ = infer(&mut gpu, &[0.; 4]);
    drop(gpu);
    assert_eq!(first[0].view().values().len(), 4);
    assert_eq!(infer(&mut cpu, &[0.; 4])[0].view().values(), &[0.; 4]);
}

// Catches ignored/rejected priority hints being advertised as applied, malformed
// driver property responses, and leaked/double-released contexts on failure.
#[test]
fn lower_priority_context_creation_and_verification() {
    let fixture = Fixture::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let executable = fixture.0.join("priority-contracts");
    let compiler = env::var_os("CXX").unwrap_or_else(|| "c++".into());
    assert!(Command::new(compiler)
        .args(["-std=c++17", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(root.join("native"))
        .arg(root.join("tests/fixtures/opencl_priority.cc"))
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap()
        .success());
    assert!(Command::new(executable).status().unwrap().success());
}

// Regresses same-handle/different-wrapper execution-context construction without
// substituting the negative CPU/no-device matrix for positive GPU coverage.
#[test]
fn execution_context_uses_its_owned_device_wrapper() {
    let fixture = Fixture::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let executable = fixture.0.join("context-device-contracts");
    let compiler = env::var_os("CXX").unwrap_or_else(|| "c++".into());
    assert!(Command::new(compiler)
        .args(["-std=c++17", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(root.join("native"))
        .arg(root.join("tests/fixtures/dnn_context_device.cc"))
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap()
        .success());
    assert!(Command::new(executable).status().unwrap().success());
}
