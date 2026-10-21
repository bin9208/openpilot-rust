# Xiaoge FP32 GPU candidate — design and implementation plan

Issue: [#453](https://github.com/bin9208/openpilot-rust/issues/453). Related CPU investigation: #416.
Base: dev-md `f060fc423a02989acc088144ebccd7454545df65`. Scope: dev-md only; no dev/main integration or #296 tool conversion.

## Goal and design

Deliver a real AGNOS test candidate that reduces resource use when GPU offload is beneficial. CPU remains the default. The owner explicitly requested implementation, an opt-in GPU switch and logs sufficient to compare detection and concurrent driving/DM timing. Device installation and driving tests remain owner-owned.

Reuse the pinned hardened OpenCV ONNX implementation, rebuilding the actual five-library AGNOS provider with OpenCL capability. This minimizes graph changes compared with a separate native QCOM or AMD export. Retain the existing ONNX assets, FP32, input sizes, thresholds, NMS, cadence/expiry and CPU placement. The GPU path must identify a real GPU, report initialization/execution/fallback honestly, and preserve owned input/output/camera lifetimes. Low GPU priority where supported is a mitigation, not proof of isolation.

Expose persistent `OnnxComputeMode`: 0=CPU (default), 1=GPU FP32, 2=GPU with sampled CPU comparison. Missing/invalid values choose CPU; HTTP accepts only the three integral values. Mode changes reload on each existing camera owner's thread. A failed GPU initialization/forward falls back explicitly to CPU and remains latched until the selection changes, avoiding repeated expensive failed initialization. GPU forwards across the two Xiaoge models serialize; CPU mode does not acquire that GPU gate.

Normal GPU mode does not run a CPU shadow. Comparison mode samples no faster than once per 10 seconds per stream, using identical prepared tensors. It records tensor errors and decoded detection differences, plus separate shadow cost, without changing production thresholds. Comparison mode is labeled as extra diagnostic load, never as normal GPU performance evidence. Logs contain summaries rather than raw images, tensors or unbounded per-layer dumps.

Use existing structured log transport and full-cereal upload path. Record selected/effective mode, source frame/timestamps, queue wait, primary and comparison wall/caller CPU costs, decoded results and bounded comparison summaries. A once-per-second resource record measures the whole process CPU clock (all workers) and RSS, distinct from caller-thread timing. GPU usage and temperature reuse existing deviceState records; Xiaoge does not add a second reader of the KGSL gpubusy sysfs counter. Existing modelV2/driverStateV2 and model/DM diagnostics supply concurrent timing; an offline Rust reader joins full-cereal records by monotonic time and reports distributions, invalidity and frame gaps. Missing data never establishes success; GPU counter zero does not establish spare capacity.

## Delivery and validation

All functional tests, native builds, ARM/AGNOS builds, sealing, installer checks and release publication run on GitHub-hosted Actions. Local work stays at syntax/format/diff and retained evidence analysis. Preserve all five required contexts and separately verify the actual merged source. Rebuild and hash/seal the new provider; baseline reuse remains limited to unaffected companions. Publish the exact-source dev-md prerelease through the existing installer. Do not claim Adreno execution, detection equivalence, device savings or absence of driving delay from hosted CPU/QEMU results.

## Implementation plan

> For agentic workers: execute these scoped tasks in parallel using explicitly selected GPT-6.1 Sol workers; the parent integrates Xiaoge policy, tests and delivery.

- [x] Native adapter: add `DnnTarget::{Cpu,OpenClFp32}`, `DnnNet::load_with_target` and truthful backend/device diagnostics; CPU `load` remains compatible. Test rejection of unsupported/CPU OpenCL and preserve existing ownership/exception contracts.
- [x] Provider/package: explicit OpenCL build option, positive capability receipt, fresh genuine-AGNOS five-library build and sealed staging; strict bounded source policy and tests preventing old CPU provider reuse.
- [x] Mode/executor: tests for absent/invalid/default/persistent mode, fallback latching, explicit re-selection, GPU serialization and bounded comparison schedule; implement owner-thread mode reload and same-input comparisons.
- [x] UI/telemetry: existing web settings selector, explicit backend/error status, structured bounded detection/resource/comparison logs and paired Korean/English docs. Preserve source-compatible baseline status when CPU/default and adapt focused tests to new optional settings.
- [x] Log reader: synthetic full-cereal mixed-mode/invalid/missing/truncated fixtures, streamed plain/zstd parsing, per-mode timing/result/resource summaries with explicit evidence limits.
- [ ] Hosted validation: focused contracts then affected native CPU comparisons on host/ARM and unsupported-GPU behavior, all five required PR contexts, independent review; no gate weakening or local test workaround.
- [ ] Delivery: checked PR to dev-md only, exact merged-source checks/build/seal/publish/public installation receipts; owner installation/drive acceptance remains open.

## Review focus

CPU mode must not initialize OpenCL; a GPU target request must not masquerade as actual GPU execution. Switching/fallback must not reuse a stale GPU result. Duplicate workers must not submit GPU inference together. Comparison must not silently add CPU work to normal GPU mode. Logging/reader failures must not stop inference or convert missing timing into a passed safety claim. Camera pixels remain owned before any callback work. Warm-up, tuning, mode transitions, queue wait, per-layer CPU fallback and diagnostic shadow load must stay visible in measurements.

## Evidence and current state

Implementation is in [PR #459](https://github.com/bin9208/openpilot-rust/pull/459). Independent review at `4890d9c646232e393893d2a86e11861e58753170` found no remaining P1/P2 items after repairing context device-wrapper identity and real-library-before-alias staging. The new helpers have explicit regression tests, including genuine AGNOS/QEMU boundary execution.

Hosted focused contracts, Clippy and nine package policy tests passed at that head ([run 39572941558](https://github.com/bin9208/openpilot-rust/actions/runs/39572941558)). Prior host/ARM preflight built the fresh OpenCL provider and passed native functional tests; Clippy findings were repaired. Exact final-source required checks and genuine AGNOS packaging are pending. The mapped settings documentation gate required the additional paired settings guide update. Continue receipts in [issue #453](https://github.com/bin9208/openpilot-rust/issues/453) through actual merged-source checks, release and public installation verification.

Resource CPU measurements cover one-second windows; an initial window may straddle mode acceptance. Separate transitions/kernel warm-up from steady runs when comparing device observations; the reader emits observations without a performance verdict. This plan replaces the earlier analysis-only stopping point; the owner expressly authorized the opt-in candidate. Static AGNOS/FP32 feasibility evidence is retained in `dev-md-onnx-load-validation.md`. Actual device execution and simultaneous timing remain owner acceptance items even after candidate publication.


A final owner/worker review at `c794b8e6` found a detached-publication race: an old valid snapshot could be enqueued after a compute-mode change cleared results. Publication now retains the shared state guard until the transport owner acknowledges its actual send. The owner never acquires state or parameters, so mode/result transitions and sends share an order without a lock cycle. A substituted transport-owner regression verifies exclusion through acknowledgment and release afterward. Independent review of that patch found no remaining P1/P2 items; new-source hosted native checks and delivery receipts remain required.

The first genuine AGNOS preflight ([39573645745](https://github.com/bin9208/openpilot-rust/actions/runs/39573645745)) rejected `flags.make` because CMake elided an already implicit SDK include alias. Its retained CMake cache selected the correct target zlib header/library. The producer now validates those target cache entries and preprocesses zlib with the actual target C++ wrapper, rejecting absent, host or mixed zlib headers. Probe command, output, exit and actual header hashes are sealed with the provider. This replaces a path-spelling heuristic with target compiler evidence; ABI/install checks remain mandatory.

Hosted head `2c48296f` passed x86_64 and ARM native/original comparisons. Its sanitizer run retained a `dnn_target` fixture-build failure: the test's ASan `LD_PRELOAD` leaked into host compiler/linker subprocesses, and LSan reported their `ld.bfd/libbfd` exit allocations. Fixture compiler commands now remove only inherited `LD_PRELOAD`; all actual model children, fixture executions and fully instrumented OpenCV/JPEG library tests retain ASan, UBSan and leak detection. The original failing output is retained in Actions [39578217100](https://github.com/bin9208/openpilot-rust/actions/runs/39578217100); new-source validation is required.
