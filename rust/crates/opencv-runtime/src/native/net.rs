use super::{native_error, ready};
use crate::bridge::ffi;
use crate::{Error, Tensor, TensorView};
use std::{marker::PhantomData, path::Path, rc::Rc};

/// Explicit FP32 execution request. CPU is the default and never initializes OpenCL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DnnTarget {
    #[default]
    Cpu,
    OpenClFp32,
}

/// Evidence for the most recent forward, rather than a GPU execution guarantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnnReport {
    pub requested_target: DnnTarget,
    /// None before a successful forward, or after a failed forward.
    pub effective_target: Option<DnnTarget>,
    /// Empty for CPU requests. For GPU requests, these identify the selected GPU.
    pub device: String,
    pub vendor: String,
    pub driver: String,
    /// A verified property is a scheduling hint, not resource isolation.
    pub priority_status: String,
    /// Allocated graph layer targets, including fused layers; not executed kernels.
    /// None before a successful forward, or after a failed forward.
    pub graph_gpu_layers: Option<u32>,
    pub graph_cpu_layers: Option<u32>,
    /// `not_run`, `forward_failed`, `cpu`, `cpu_fallback`, or
    /// `opencl_active_kernel_fallback_unknown`. OpenCL layers may execute CPU code.
    pub execution_status: String,
}

/// A DNN owner confined to its creating thread; no borrowed input survives forward.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<openpilot_opencv_runtime::DnnNet>();
/// ```
/// ```compile_fail
/// fn require_sync<T: Sync>() {}
/// require_sync::<openpilot_opencv_runtime::DnnNet>();
/// ```
pub struct DnnNet {
    net: cxx::UniquePtr<ffi::Net>,
    owner_thread: PhantomData<Rc<()>>,
    last_forward: Option<bool>,
}

impl DnnNet {
    pub fn load(path: &Path) -> Result<Self, Error> {
        Self::load_with_target(path, DnnTarget::Cpu)
    }
    /// GPU opt-in is strict: no CPU OpenCL devices or disabled OpenCL providers.
    /// The caller serializes GPU load/forward across nets; CPU calls stay parallel.
    pub fn load_with_target(path: &Path, target: DnnTarget) -> Result<Self, Error> {
        ready()?;
        let name = path
            .to_str()
            .ok_or(Error::Contract("ONNX path must be UTF-8"))?;
        if name.as_bytes().contains(&0) {
            return Err(Error::Contract("ONNX path contains NUL"));
        }
        let native_target = match target {
            DnnTarget::Cpu => 0,
            DnnTarget::OpenClFp32 => 1,
        };
        let net = ffi::load_onnx_with_target(name, native_target)
            .map_err(|error| native_error("readNetFromONNX", error))?;
        Ok(Self {
            net,
            owner_thread: PhantomData,
            last_forward: None,
        })
    }
    /// Does not initialize OpenCL or bind any context. GPU identity is cached at load.
    pub fn report(&self) -> Result<DnnReport, Error> {
        let evidence = self.last_forward == Some(true);
        let native = ffi::net_report(&self.net, evidence)
            .map_err(|error| native_error("DNN report", error))?;
        report_from_native(native, self.last_forward)
    }
    pub fn output_names(&self) -> Result<Vec<String>, Error> {
        ffi::output_names(&self.net)
            .map_err(|error| native_error("getUnconnectedOutLayersNames", error))
    }
    /// Empty output names select the original default forward() result.
    pub fn forward(
        &mut self,
        input: TensorView<'_>,
        names: &[String],
    ) -> Result<Vec<Tensor>, Error> {
        self.last_forward = Some(false);
        let outcome = (|| {
            if names.iter().any(|name| name.as_bytes().contains(&0)) {
                return Err(Error::Contract("output name contains NUL"));
            }
            let outputs = ffi::forward(self.net.pin_mut(), input.values(), input.shape(), names)
                .map_err(|error| native_error("setInput/forward", error))?;
            if outputs.len() != names.len().max(1) {
                return Err(Error::Contract(
                    "DNN result count differs from requested names",
                ));
            }
            let result = outputs
                .into_iter()
                .map(|output| Tensor::from_parts(output.shape, output.values))
                .collect::<Result<Vec<_>, Error>>()?;
            Ok(result)
        })();
        if outcome.is_ok() {
            self.last_forward = Some(true);
        } else {
            ffi::invalidate_net_report(self.net.pin_mut());
        }
        outcome
    }
}

fn report_from_native(
    native: ffi::DnnReportData,
    last_forward: Option<bool>,
) -> Result<DnnReport, Error> {
    let evidence = last_forward == Some(true);
    let requested_target = match native.requested_target {
        0 => DnnTarget::Cpu,
        1 => DnnTarget::OpenClFp32,
        _ => return Err(Error::Contract("unknown native DNN target")),
    };
    let effective_target = if evidence {
        Some(if native.opencl_active && native.graph_gpu_layers > 0 {
            DnnTarget::OpenClFp32
        } else {
            DnnTarget::Cpu
        })
    } else {
        None
    };
    let execution_status = match (last_forward, requested_target, effective_target) {
        (None, _, _) => "not_run",
        (Some(false), _, _) => "forward_failed",
        (_, DnnTarget::Cpu, _) => "cpu",
        (_, _, Some(DnnTarget::Cpu)) => "cpu_fallback",
        _ => "opencl_active_kernel_fallback_unknown",
    };
    Ok(DnnReport {
        requested_target,
        effective_target,
        device: native.device,
        vendor: native.vendor,
        driver: native.driver,
        priority_status: native.priority_status,
        graph_gpu_layers: evidence.then_some(native.graph_gpu_layers),
        graph_cpu_layers: evidence.then_some(native.graph_cpu_layers),
        execution_status: execution_status.into(),
    })
}

#[cfg(test)]
mod tests;
