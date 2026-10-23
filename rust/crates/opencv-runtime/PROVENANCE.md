# External OpenCV boundary

Native dependency is OpenCV4.13.0 source revision `fe38fc608f6acb8b68953438a62305d8318f4fcd`, downloaded archive SHA256 `6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61`. Original Python oracle is opencv-python-headless4.13.0.92, revision `b4c5ec4042f097e2a5b386b9d413ec7333d0a184`; its parent is the pinned native revision. Retained comparison shows only two Python test-file edits, without native source edits.

The native build applies `native/opencv-defined-access.patch`, SHA256
`7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec`.
Full-library UBSan exposed unaligned scalar loads in SSE lookup helpers and
negative signed-coordinate left shifts in `CollectPolyEdges`. The patch copies
the same scalar bytes with `memcpy` and multiplies signed 64-bit coordinates by
the same positive fixed-point scale. Validated i32 caller coordinates fit that
wide product. C++11 and all sanitizer checks remain enabled. The builder verifies
the exact original and patched hashes of both affected files, and freezes the
patch, hashes, and modified sources alongside the installed library manifest.
The original wheel oracle remains unchanged. Unpatched library trees and RED
traces are retained separately; patched outputs never replace their libraries.
The checksum-pinned unified diff retains its blank context-line whitespace;
only that patch artifact is exempted from Git whitespace diagnostics.

`cargo run --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin build-xiaoge-opencv -- --output PROVIDER` builds only core/imgproc/dnn, without Python, IPP, OpenCL, LAPACK, GUI, codecs, or CUDA. OpenCV retains its Apache2.0 license; bundled protobuf retains BSD3-Clause, flatbuffers Apache2.0 and zlib1.3.1 its Zlib license. Builder copies complete upstream licenses into its output and freezes library/header/license SHA256 identities in `native-libraries.sha256` and `receipt.json`. Cargo requires `OPENPILOT_OPENCV_ROOT` and verifies that manifest before compiling CXX. No cv2.so or Python runtime is linked.

The CXX declarations accept borrowed slices, validate all extent/product/format contracts twice, and return owned Rust vectors. The pinned `Net::Impl::setInput` synchronously copies the borrowed FP32 input into its own `inputsData` before returning; the adapter does not make an additional clone or retain the Rust pointer. Reassess that provider contract when updating OpenCV. Exceptions are caught by CXX and returned as typed Rust errors. DnnNet has no Send/Sync implementation and contains an Rc marker. Once-only thread initialization completes before any native operation, avoiding concurrent changes to OpenCV's global thread configuration.

Crop selection, polygon scaling, normalization, channel ordering, confidence interpretation, scheduling and control remain in Rust callers. The adapters perform external OpenCV calls plus representation/ownership conversion only. The checked strided grayscale ROI resize and NV12 conversion/mask/resize composition avoid intermediate owned copies while retaining the same operation order and output pixels. JPEG remains a separate external dependency.

Pure contract tests run with `--no-default-features` under Miri. They verify checked dimensions, lengths, shape products and owned/borrowed views; Miri cannot execute the native library. Native exception/ownership checks, exact pixel differentials and real pinned-model comparisons require actual CXX execution with ASAN/UBSAN coverage. Host and aarch64 GNU/QEMU evidence must remain separate; GNU symbol/version and AGNOS sysroot compatibility require explicit checks, and none establishes camera/device acceptance.

## CPU-default FP32 GPU candidate (#453, 2026-10-21)

The external source revision and hardening patch above are unchanged. The
historical provider described above had `WITH_OPENCL=OFF`. A GPU-capable
candidate must build that same source with `-DWITH_OPENCL=ON`, keeping the
existing core/imgproc/dnn module selection, FP32 models, and other provider
constraints. `ENABLE_INSTRUMENTATION` is **not required**. The adapter links
`libdl`; it has no link-time OpenCL dependency. The stock device `libOpenCL.so`
(or `.so.1`) is loaded dynamically only for an advertised Qualcomm priority
extension. The Linux x86_64/aarch64 ABI declarations in `native/opencl_priority.h`
cover only context creation, property query and release. No vendor implementation
or Qualcomm header is redistributed by this change.

### Public contract

`DnnTarget::{Cpu, OpenClFp32}`; `DnnNet::load_with_target(&Path, DnnTarget)`;
`DnnNet::report(&self) -> Result<DnnReport, Error>`. Existing `load` delegates to
CPU. `DnnReport` fields are:

| Field | Rust type | Meaning |
| --- | --- | --- |
| requested_target | DnnTarget | This net's load request; a parent CPU reload is a separate net |
| effective_target | Option<DnnTarget> | Last successful graph/runtime state; None before forward or after failure |
| device, vendor, driver | String | Selected real GPU identity; empty for CPU requests |
| priority_status | String | not_applicable_cpu, unsupported_qcom_priority_hint, or qcom_low_requested_verified |
| graph_gpu_layers, graph_cpu_layers | Option<u32> | Allocated graph layer targets including fused layers; None without a successful forward |
| execution_status | String | not_run, forward_failed, cpu, cpu_fallback, or opencl_active_kernel_fallback_unknown |

The parent maps `Some(Cpu)` to `cpu`, `Some(OpenClFp32)` to `opencl-fp32` and
`None` to an unavailable effective state. On strict GPU load errors the parent
owns CPU reload and requested/effective/reason logging. With a GPU request,
zero allocated OpenCL layers or an inactive OpenCL runtime reports CPU fallback.
Neither an active context nor a graph OpenCL target proves successful GPU kernel
execution, and an OpenCL-targeted layer can fall back internally to CPU. No actual
per-layer kernel count or absence of CPU fallback is claimed.

Graph target counts use the public `cv::dnn::Layer::preferableTarget` field,
which pinned `Net::Impl::dump` also reads. They are cached after the first
successful allocated forward and refreshed for changed input shape/output names
or after native forward failure. A failed Rust-side validation also hides all
previous execution evidence. `report()` only copies cached strings/counts; it
does not dump or walk a graph or trigger graph allocation. Fixed-shape callers can cache a report
and refresh at 1 Hz, handling forward errors immediately. Native queue activity
is finished before the GPU forward returns or an exception releases the caller's
lock. Kernel placement remains unknown between and at report refreshes.

CPU load/forward disable OpenCL with `setUseOpenCL(false)` and inspect only
`OpenCLExecutionContext::getCurrentRef()` (the noninitializing TLS accessor).
They never call `haveOpenCL`, `useOpenCL`, `getCurrent`, or bind a context. An
existing context's enable state is restored afterward. On a previously empty
CPU thread, OpenCL remains disabled. GPU calls bind a per-net execution context,
restore any prior binding/enable state, and explicitly clear an initially empty
binding; stock `OpenCLExecutionContextScope` does not restore that empty case.
GPU net destruction also releases graph allocations under its own context,
then restores the caller. The adapter creates no worker threads or global GPU lock. The parent serializes
GPU loads/forwards across nets; CPU nets remain parallel. OpenCV's synchronous
input copy, owned output copies, and the Rust Rc ownership marker retain the
ownership boundary.
There are no FP16 targets, model rewrites, or process scheduling changes.

GPU selection enumerates public OpenCV platform/device APIs, requires exact
`Device::TYPE_GPU`, availability and a compiler, prefers a device advertising
`cl_qcom_priority_hint`, and rejects CPU OpenCL devices. It validates an active
matching context/queue and OpenCV DNN's advertised OpenCL FP32 target before
accepting load. Unsupported OpenCL builds/runtimes return an error. OpenCV's DNN
backend registry caches advertised targets at first query; another consumer that
initializes that registry with OpenCL disabled can cause a conservative GPU-load
rejection rather than a falsely accepted GPU. The parent can log the error and
remain on CPU.

If the priority extension is advertised, context creation requests low priority
and verifies its retained property with `clGetContextInfo` before importing it
through public `Context::fromHandle` and `OpenCLExecutionContext::create` APIs.
A rejected/missing property, import error or missing dynamic symbol fails GPU
load for the caller to recover to CPU. If the extension is absent, a regular GPU
context is allowed with explicit `unsupported_qcom_priority_hint` status. The
verified property is a **hint**, not proof of scheduler enforcement, hard GPU
isolation, preemption, reserved resources or driving-model/DM protection.

### Source verification

- Public APIs and exact CPU/TLS behavior: OpenCV revision
  `fe38fc608f6acb8b68953438a62305d8318f4fcd`,
  [ocl.hpp](https://github.com/opencv/opencv/blob/fe38fc608f6acb8b68953438a62305d8318f4fcd/modules/core/include/opencv2/core/ocl.hpp),
  [ocl.cpp](https://github.com/opencv/opencv/blob/fe38fc608f6acb8b68953438a62305d8318f4fcd/modules/core/src/ocl.cpp),
  and [net_impl.cpp](https://github.com/opencv/opencv/blob/fe38fc608f6acb8b68953438a62305d8318f4fcd/modules/dnn/src/net_impl.cpp).
- Linux OpenCL ABI/types and `CL_CONTEXT_PROPERTIES=0x1082`: Khronos OpenCL-Headers
  `v2024.10.24`, [cl.h](https://github.com/KhronosGroup/OpenCL-Headers/blob/v2024.10.24/CL/cl.h),
  SHA256 `3c51949c38bf9bca6dbe919a1f60727bad1ef4a7a321c6d530f7502ddc1fd706`;
  [cl_platform.h](https://github.com/KhronosGroup/OpenCL-Headers/blob/v2024.10.24/CL/cl_platform.h),
  SHA256 `be9647d121bfea20cfac9fe2be2e3ac7851a0dd77a141363dcc72f126a1a358c`.
- Qualcomm's SDK declarations retained in commaai/openpilot revision
  `8aadf02b2fd91f4e1285e18c2c7feb32d93b66f5`:
  [cl_ext_qcom.h](https://github.com/commaai/openpilot/blob/8aadf02b2fd91f4e1285e18c2c7feb32d93b66f5/third_party/opencl/include/CL/cl_ext_qcom.h),
  SHA256 `11c26633f533f14f5ffac1cc227932e251d6ad86fcac4c7f4e217f046a6e35c5`.
  Verified `CL_CONTEXT_PRIORITY_HINT_QCOM=0x40C9` and
  `CL_PRIORITY_HINT_LOW_QCOM=0x40CC`; no guessed extension constants.
  Qualcomm's [OpenCL optimization guide, section 9.1.2](https://docs.qualcomm.com/bundle/publicresource/80-NB295-11_REV_C_Qualcomm_Snapdragon_Mobile_Platform_Opencl_General_Programming_and_Optimization.pdf)
  documents the context-creation hint semantics. The performance/frequency hint
  `cl_qcom_perf_hint` is different and is not requested here.
- Instrumentation was inspected, not enabled: pinned `core/src/ocl.cpp`
  `Kernel::Impl::run` enters `CV_INSTRUMENT_REGION_OPENCL_RUN` before checking
  the kernel handle or successful enqueue; `core/src/system.cpp`
  `IntrumentationRegion::~IntrumentationRegion` calls `ocl::finish` in OpenCL
  function/wrapper regions. A raw IMPL_OPENCL call counter includes attempts
  and enabling it changes synchronization/timing. It is not used as execution
  proof or included in comparison measurements.

### Candidate checks (GitHub Actions execution required)

Tests were written before implementation; local functional RED/GREEN, library
builds, installs and device access are explicitly forbidden for this task.
Only Rust formatting, C/C++ syntax checks and diff review are performed locally.
The following are authored checks, not passing runtime/device evidence:

- `tests/dnn_target.rs`: synthetic Conv+Relu ONNX (hand-specified FP32 expected
  output), CPU-default/explicit-CPU equivalence and output lifetime, failed-forward
  evidence invalidation/recovery, fresh-process loader probe proving CPU never
  loads the OpenCL runtime, and strict no-device GPU rejection.
- `tests/fixtures/opencl_priority.cc`: standalone mock context-API tests verify
  the exact low-priority request, retained and reordered properties, rejected or
  ignored/wrong hints, malformed/query-error responses, and exactly-once release
  on success and failure. Rust's native test compiles/runs this helper in Actions;
  C/C++ compilers must be available there. No mock is presented as GPU evidence.
- `src/native/net/tests.rs`: report translation distinguishes a global CPU
  fallback from OpenCL-active graph placement and keeps kernel fallback unknown;
  stale success cannot leak through failed/not-yet-run reporting.
- Native doctests enforce DnnNet's lack of Send/Sync.
- In Actions, run native tests with `OPENCV_TEST_GPU=reject` for both an
  OpenCL-disabled provider and an OpenCL-enabled provider with only a CPU ICD.
  Run with `OPENCV_TEST_GPU=require` only on an actual GPU validation runner:
  this additionally requires strict GPU acceptance, the expected graph/report,
  FP32 synthetic outputs and GPU/CPU/GPU ownership/context-switch behavior.
  With that variable absent, hardware-specific acceptance is not exercised.

Real-model CPU/GPU comparisons, recording/upload startup, timing joins against
modeld/DM, installation and real-device/drive acceptance belong to the parent
integration and cloud/device validation track. No speedup or isolation is
established by this adapter candidate.

### Review repair: context-owned Device wrapper (#453)

The initial GPU construction passed the enumerated `selected` Device to
`OpenCLExecutionContext::create(context, selected)`. This is invalid for pinned
OpenCV when context creation/import has constructed a distinct Device wrapper
around the same `cl_device_id`. In pinned
[ocl.cpp](https://github.com/opencv/opencv/blob/fe38fc608f6acb8b68953438a62305d8318f4fcd/modules/core/src/ocl.cpp),
`OpenCLExecutionContext::Impl::_init_device(const ocl::Device&)` compares
`d.getImpl() == device.getImpl()`, whereas `Context::Impl::init_device_list`
constructs device wrappers with `Device::fromHandle(cl_devices[i])`.
A raw-handle match alone does not satisfy that overload's wrapper identity check.

Both the Qualcomm imported-context path and ordinary `Context::fromDevice` path
now pass through `create_context_execution`: reject empty/non-single-device
contexts, verify the context-owned device has a non-null raw handle equal to the
selected GPU's handle, then pass **context.device(0)** to the public execution
factory. Existing selected-GPU validation, context/queue checks, target precision,
priority behavior and Rust API remain unchanged. The helper is a private,
header-only factory boundary so its production argument selection can be tested
without a GPU or an extra OpenCL implementation dependency.

The new `tests/fixtures/dnn_context_device.cc` regression models the pinned
factory's wrapper identity check: the old enumerated-device argument fails with
same-handle/different-wrapper input, while the actual production helper passes
the context-owned wrapper. It also checks already-matching wrappers and rejection
of empty, zero-device, multi-device, mismatched-handle and null-handle contexts
before factory invocation. `tests/dnn_target.rs` compiles/runs this fixture only
when native tests execute in Actions, like the existing priority boundary tests.
This is a mocked adapter-boundary regression, **not** an actual OpenCV positive
GPU construction, kernel execution, or device acceptance result. Earlier
host/ARM CPU/no-GPU rejection passes could not exercise this positive path.
Local repair verification is restricted to rustfmt, C++ syntax and diff review;
functional RED/GREEN and the repaired native provider checks remain for Actions.
