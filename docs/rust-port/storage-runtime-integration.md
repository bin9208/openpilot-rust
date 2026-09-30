# Storage, torque, Jetlink and diagnostics integration

Issue [#56](https://github.com/bin9208/openpilot-rust/issues/56) integrates four
independently reviewed increments from dev
`8604a879283210e58764c7e3a0ea9dc84404fb70`. Original feature histories are retained.

| Increment | Reviewed feature revision | Evidence and limits |
| --- | --- | --- |
| Torque estimator and continuous daemon #40 | `2b6258e0a7b57024b68887217ad7ce2814f19496` | [Original NumPy/OpenBLAS and IPC comparisons](torqued-validation.md) |
| Route storage, media and diagnostics #41 | `557b371e4273870d2a2e3b01cc04b1f0658eab29` | [Full-schema, actual cloudlog/collector, media and ENOSPC comparisons](loggerd-validation.md) |
| Jetlink owner, RPC and model selection #44 | `05fcec3c3f6d9e3f25d3b0a704c7524653fbf127` | [Original policy/protocol, real RPC and model input comparisons](jetlink.md) |
| Common structured logging and diagnostics #45 | `e4c9cad2cfbc0ab810d0e5937aa6120670fb5e43` | [Python/native producer, runtime metrics and transport comparisons](logging-client-validation.md) |

Shared Cargo workspace entries, native FFmpeg prerequisites, torque numerical
artifacts and CI steps are combined. The required Rust gate retains host, native
model pipeline, memory, route logger and ARM dependencies. Inherited fast,
integration and mapped documentation checks remain required. Native route tests
also run the shared C++ logging oracle and actual original/Rust collector paths.

All model/reference jobs now use the source-locked NumPy 2.5.3 dependency. Source
models and native catalogs must be regenerated under that environment; old
NumPy 2.4.6 artifacts do not prove the combined revision. The torque numerical
loader validates the locked wheel hash and dependency identity for its target
architecture. Native FFmpeg and NumPy/OpenBLAS are explicitly recorded external
dependencies; generic ARM artifacts are not AGNOS deployment evidence.

Parent reviews and native QA are preserved in the local archive and individual
validation documents. Exact combined PR and post-merge Actions links are added
to #56 before any bounded issue is closed. This document does not predeclare
pending cloud checks as successful.

The first combined PR run, `384e8098`, stopped the new logger job before its
first Rust test: rustup's automatic component installation reported a conflict
for `bin/cargo-fmt`. The job now requests rustfmt/clippy with its initial pinned
toolchain installation, matching the established host job and
`rust/rust-toolchain.toml`. The failed run remains evidence; final-head checks
must validate this change.

The same run exposed [#59](https://github.com/bin9208/openpilot-rust/issues/59):
the torque scheduler's single-field `sched_param` literal compiled on GNU but
failed on musl's larger structure. A local aarch64 musl check reproduced E0063.
Initializing the complete structure before assigning priority 5 follows the
existing calibration/Jetlink boundary and preserves FIFO policy and affinity.
The corrected torque executable builds for aarch64 musl; exact-head cloud gates
still need to validate the complete workspace.

The next run at `59e836c8` passed model memory checks and exposed
[#62](https://github.com/bin9208/openpilot-rust/issues/62): the native logging
oracle changed directory before resolving caller-relative executable paths.
Resolving both input and output paths first fixes the invocation from `rust/`.
The actual C++/Rust comparison then passed with relative CLI arguments (168
wire records, 516 rate inputs and backpressure/thread/console/signal checks).
An initial local rerun correctly rejected an older binary's commit identity;
rebuilding the probe from the current revision resolved that provenance error.

At `7bf70206`, the PR's required Rust gate passed, while its push counterpart
timed out in the original DEBUG-capacity crash fixture after five seconds
([#67](https://github.com/bin9208/openpilot-rust/issues/67)). The failure log
did not identify whether import, send or crash collection took that time.
On the local Linux host, the unchanged oversized packet asserted identically
in all ten diagnostic runs. With only `RLIMIT_CORE=0`, five aborts took
1.157–1.164 seconds; with process-local `PR_SET_DUMPABLE=0`, five took
0.073–0.076 seconds. The local host uses an external core-dump handler; this
measurement establishes that disabling the resource limit alone does not
remove that handler's latency, not the exact cause of the remote timeout.
The fixture now disables dumpability only in its intentional-crash child,
records import/send milestones and retains timeout stdout/stderr. The same
five-second watchdog, original SIGABRT, oversized packet, queue capacity and
Rust error assertions remain. The complete affected native fixture passes
locally; exact updated-head CI is required before integration.

Full-runtime #1/#6 remain open. The remaining daemon ports, production startup,
model diagnostic callsites (#53), driving startup-order parity (#55), active web
upload orchestration, and device/drive acceptance are separate requirements.
Inherited source defects #46/#48/#49/#54 remain tracked independently. This
integration does not change production selection or claim vehicle/CPU results.

Docs-Not-Needed: internal runtime integration; no user setting or production
selection change.
