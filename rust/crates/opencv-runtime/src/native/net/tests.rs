use super::{report_from_native, DnnTarget};
use crate::bridge::ffi::DnnReportData;

fn gpu_request(gpu: u32, cpu: u32, active: bool) -> DnnReportData {
    DnnReportData {
        requested_target: 1,
        device: "test GPU".into(),
        vendor: "test vendor".into(),
        driver: "test driver".into(),
        priority_status: "unsupported_qcom_priority_hint".into(),
        graph_gpu_layers: gpu,
        graph_cpu_layers: cpu,
        opencl_active: active,
    }
}

// Catches requested-target reporting hiding native global CPU fallback even
// while the selected OpenCL context itself remains enabled.
#[test]
fn global_cpu_fallback_is_not_reported_as_requested_gpu() {
    let report = report_from_native(gpu_request(0, 12, true), Some(true)).unwrap();
    assert_eq!(report.requested_target, DnnTarget::OpenClFp32);
    assert_eq!(report.effective_target, Some(DnnTarget::Cpu));
    assert_eq!(report.graph_gpu_layers, Some(0));
    assert_eq!(report.graph_cpu_layers, Some(12));
    assert_eq!(report.execution_status, "cpu_fallback");
    let disabled = report_from_native(gpu_request(12, 0, false), Some(true)).unwrap();
    assert_eq!(disabled.effective_target, Some(DnnTarget::Cpu));
    assert_eq!(disabled.execution_status, "cpu_fallback");
}

// Neither allocated OpenCL layers nor a live GPU context proves kernel placement.
#[test]
fn active_graph_explicitly_keeps_kernel_fallback_unknown() {
    let report = report_from_native(gpu_request(10, 2, true), Some(true)).unwrap();
    assert_eq!(report.effective_target, Some(DnnTarget::OpenClFp32));
    assert_eq!(report.graph_gpu_layers, Some(10));
    assert_eq!(report.graph_cpu_layers, Some(2));
    assert_eq!(
        report.execution_status,
        "opencl_active_kernel_fallback_unknown"
    );
}

#[test]
fn failed_or_unrun_forward_never_reuses_native_success_evidence() {
    for (state, status) in [(None, "not_run"), (Some(false), "forward_failed")] {
        let report = report_from_native(gpu_request(10, 2, true), state).unwrap();
        assert_eq!(report.effective_target, None);
        assert_eq!(report.graph_gpu_layers, None);
        assert_eq!(report.graph_cpu_layers, None);
        assert_eq!(report.execution_status, status);
    }
}
