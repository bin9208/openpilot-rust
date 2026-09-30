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

Full-runtime #1/#6 remain open. The remaining daemon ports, production startup,
model diagnostic callsites (#53), driving startup-order parity (#55), active web
upload orchestration, and device/drive acceptance are separate requirements.
Inherited source defects #46/#48/#49/#54 remain tracked independently. This
integration does not change production selection or claim vehicle/CPU results.

Docs-Not-Needed: internal runtime integration; no user setting or production
selection change.
