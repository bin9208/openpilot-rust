# Supporting runtime integration

Issue [#71](https://github.com/bin9208/openpilot-rust/issues/71) integrates the
validated support services after [storage integration #56](storage-runtime-integration.md).
The parent [full-runtime design](design.md) remains the delivery contract: finish
the project-owned runtime, normal startup and existing upload path before the
first user device comparison. This integration does not select native daemons in
the production manager or establish device performance.

## Frozen component inputs

| Component | Reviewed input | Evidence |
| --- | --- | --- |
| Active upload jobs, metadata and worker ownership (#61), including HTTP #52, uploader #50 and STRING Params #64 | `292bc8c67df2b8e1d9f767d8ba16f1fa0e44bc1b` | [Dashcam](dashcam-jobs-validation.md), [transport](web-upload-validation.md), [uploader](uploader-validation.md), [typed Params](../naver/rust_params_string_20260930.md) |
| Uploader logging error boundaries (#68) | `4efeb5a4db0e753aac6fbf7d562bf3396bba2464` | [Fault comparison](../naver/rust_uploader_logging_errors_20260930.md) |
| Model diagnostics and startup (#53/#55/#60) | `1cdb61820c2356b6e43b73b7c7880ad2c3df9dad` | [Diagnostics](model-diagnostics.md), [startup](model-startup.md) |
| Native journal child and JSON bridge (#65) | `26942c5d8206693a4447e9f481575f215b7c894b` | [Journal source comparison](journald-validation.md) |
| Clock/timezone daemon and shared socket timeout transport (#70), including local datetime range correction | `3e42cd820d82f71a136f788c184c3bd2e1fe94b8` | [Clock, HTTP, date and shutdown comparison](timed-validation.md) |

Component host/ASan/ARM records retain their own source and executable hashes.
Those results do not substitute for the combined revision's checks. Dependency
resolution combines both parent lockfiles; all external package versions and
checksums match a frozen parent. Model and torque comparisons retain NumPy 2.5.3.

## Integration validation

The workflow must retain all inherited fast, integration, mapped-documentation,
Rust and aarch64 requirements. The supporting runtime comparisons run in a
separate required job so the existing near-20-minute host job does not absorb
additional network deadline and process-lifecycle scenarios. The gate must reject
failed, skipped or missing prerequisite results.

Required new comparisons include actual Cython Params reads and cast warnings,
uploader diagnostics and logging transport failures, all six dashcam helper and
worker-lifecycle checks, the journal bridge and child cleanup, and model startup
before frames/CarParams. Original msgq and VisionIPC bindings serve as independent
peers. Raw capture artifacts are retained when a check fails.

The first combined source revision `dc9865b3` passes workspace formatting,
warnings-denied Clippy, 256 Rust tests (none ignored), and all binary/example
builds. Native runs of the proposed support CI commands pass the uploader,
web-upload, Cython Params, logging/error, dashcam helper/HTTP/lifecycle and journal
comparisons. The initial local attempts identified missing Cython/setuptools,
Cap'n Proto headers and pyserial in the local oracle environment; the corrected
environment uses the same dependencies declared by CI. Failed attempts remain in
the local evidence alongside the completed comparisons.

Those captures precede the test-only Jetlink socket correction merged from
`6218d397`. At the resulting `af181858`, fresh original VisionIPC/model runs also
pass all eight initialization scenarios, continuous driving and driver monitoring,
and logging-failure recovery while inference continues. Their retained initial
fault-check attempt lacked Cargo in the local PATH; adding the existing toolchain
path allowed the unchanged check to complete.

The clock/timezone review reproduced two native mismatches before accepting
`d929a9c4`: JSON duplicate/nonfinite handling, and roughly 9–10-second shutdown
delays during GPS sleep or progressing HTTP. The corrected native daemon preserves
normal timeouts and handles stop promptly. Parent execution independently confirms
all six real shutdown scenarios and verifies 1,635 frozen evidence hashes. A
QEMU-specific missing-command spawn discrepancy remains explicitly recorded;
the ARM policy audit passes 53 of 54 cases and is not described as full parity.

The combined workspace at `3bb36bd8` passes formatting, warnings-denied Clippy,
259 Rust tests (none ignored), and binary/example builds. The support command
run passes its first eight stages, covering the upload, Params, diagnostics and
dashcam scenarios, before exposing a reentrant buffered append in the synthetic
journal child trace. [Fixture correction #74](../naver/rust_journal_trace_74.md)
reproduces that ordering failure deterministically and fixes only the fixture;
the actual original/native journal comparisons and all 19 configured tool tests
then pass. Production journal behavior and assertions are unchanged.

The final clock review also finds that Chrono accepts local years outside the
Python datetime domain. The correction checks both the localized result and
the localized 24-hour fold probe, preserving timezone crossings that remain
representable. Expanded source comparisons cover 360 date cases and 16 actual
daemon executions. Both checks are required in the supporting runtime CI job.
The old failing cases and corrected observations remain in the clock evidence.

One later local journal attempt used an earlier executable after the integration
merge and correctly failed its embedded `source_commit` assertion. Final
integration execution requires rebuilding from the revision under test; this
attempt is retained as a provenance failure rather than a runtime mismatch.

Exact-SHA Actions and post-merge results are still pending. PR success,
post-merge success, device execution and user acceptance remain separate states.

## Remaining runtime work

The manager/startup path and active Carrot HTTP server are still unported. Camera,
encoder, hardware management, navigation, vehicle interfaces, planning, radar,
control/state management, UI and remaining model backends also remain in the
inventory. The existing C++ msgq/VisionIPC boundary is a retained project-owned
implementation requiring later conversion; external codecs, OS interfaces,
model kernels and numerical libraries are recorded separately.

Docs-Not-Needed: internal Rust integration and validation; no production settings
or selected daemon behavior changes.
