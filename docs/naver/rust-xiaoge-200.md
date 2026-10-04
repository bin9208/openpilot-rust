# Native Xiaoge runtime: issue 200

The Xiaoge broadcaster, lane/BSD processing and diagnostic HTTP service have a
Rust runtime candidate. Host process comparisons and ARM numerical comparisons
pass. Hosted CI and complete manager startup/upload remain integration gates.
This is a stage of [issue 1](https://github.com/bin9208/openpilot-rust/issues/1),
tracked in [issue 200](https://github.com/bin9208/openpilot-rust/issues/200).
No vehicle was connected or tested, and production process selection is unchanged.

## Implementation and dependencies

`rust/crates/xiaoge` retains the original `xiaoge_data.py`, `v_asm_server.py`,
`v_asm_inference.py`, `lane_inference.py`, `nv12.py` and `xiaoge_vision.py`
policy. This includes the 20 Hz TCP packet stream, latched Tesla CAN selection,
freshness and lane-change gates, model preprocessing/postprocessing, original
ONNX assets, Params refresh/persistence, diagnostic publications, snapshots,
HTTP routing/error bodies and signal behavior. Its Rust code forbids unsafe code.
The original HTML is served unchanged; there is no Python runtime fallback.

The explicit `native-skip-miri` feature links the Rust msgq/VisionIPC transport,
the separate OpenCV CXX adapter and JPEG adapter. OpenCV 4.13.0's core/imgproc/dnn
and libjpeg-turbo remain external native dependencies. The OpenCV source,
libraries, headers, patches and licenses are pinned and verified before linking.
See `rust/crates/opencv-runtime/PROVENANCE.md` and
`rust/crates/jpeg/PROVENANCE.md` for ownership and licensing boundaries.

Full-library UBSan found original OpenCV scalar SSE alignment and negative signed
shift issues, tracked in [203](https://github.com/bin9208/openpilot-rust/issues/203)
and [204](https://github.com/bin9208/openpilot-rust/issues/204). The small pinned
defined-access patch retains exact image/model results against the unchanged wheel.
Original failing libraries and traces remain preserved. A valid long VisionIPC
namespace exposed a Rust path-length discrepancy, fixed in
[202](https://github.com/bin9208/openpilot-rust/issues/202). The full socket path
still must fit the native Unix-address limit. A directory or empty `CarParams`
now retains the original retry behavior; recovery is tracked with Radar in
[205](https://github.com/bin9208/openpilot-rust/issues/205).

## Observed verification

The executed host daemon SHA256 is
`26a5aee80cacc90f39638906e26f263ae139ea391fff5958d70fe613f3363d60`.
It is frozen with five real probes and a source capsule in
`.omo/evidence/xiaoge-200/runtime-frozen-v6/` in the issue worktree.
`full-ci-host-v1/` records the complete reusable CI recipe and exact commands.

| Surface | Observed result |
| --- | --- |
| Settings, gates and image-layout policy | 436 unchanged-source cases match. |
| Lane postprocessing | 14 source cases match complete candidates and results. |
| Complete actual model pipeline | Four padded NV12 frames, 24 exact image/tensor files and complete lane/BSD results match. |
| External OpenCV | 22 cases match exact pixels, output shapes/names and every float32 model-output bit. |
| JPEG | 16 source/Pillow cases match bytes for legacy RGB75, RGB85 and grayscale50 snapshots. |
| Actual HTTP and TCP | 65 source/native cases match, including raw request paths, malformed bodies/lengths, Params effects, snapshots and heartbeat packets. |
| Actual Cereal/CAN/VisionIPC | 19 source/native phases pass, including directory/empty Params recovery, both BSD directions, stale/invalid input, latched brand, settings/config refresh and camera restart. |
| Process lifecycle | Ten source/native cases pass: occupied ports, missing models, malformed config, fatal Params root, incomplete clients, 32 reconnects and signals. |
| Browser | Real Chrome validates settings apply, snapshot refresh, region restore/save and three languages; desktop and 390px mobile captures are retained. |
| Native memory | Six executed test binaries plus all 22 OpenCV and 16 JPEG comparisons pass ASan/UBSan/leak detection with complete external libraries instrumented. |
| Pure memory contracts | Six actual pure contract tests pass four pinned Miri modes. Native FFI is covered separately by execution. |
| Rust checks | 33 tests, all-target builds and strict Clippy pass; catalog registration has a failing-before/passing-after regression. |

Live source and Rust processes use independent original C++ input publishers,
actual model assets and full cereal schemas. Timing/counter fields are retained
but not asserted cross-process equal; complete stable payloads, validity and
snapshot bytes are compared. The browser uses synthetic frames and an owned
profile. The unchanged page can overwrite unfocused unsaved form fields during
polling; each tested setting was applied immediately. It is not a UI redesign.

The ARM daemon SHA256 is
`13f10335ee35ba1656b7de1556d63ef690c4d117d9f86994d0da9077db8df1ed`,
frozen in `runtime-arm-frozen-v1/`. Sixteen test executables pass all 33 assertions
under Cortex-A57 emulation with the retained actual AGNOS19.8 loader/libraries.
Four full inference frames/24 binary intermediates, 14 lane cases and 22 OpenCV
cases match actual ARM CPython/NumPy/OpenCV source results exactly. The recursive
14-file AArch64 dependency and symbol-version closure passes. Host and ARM model
results can differ; each native result is compared to its own architecture's
original result, without widening tolerances.

## Repeatable gate and remaining integration

`check_xiaoge_ci.py` executes all nine source/runtime lanes. The required
`xiaoge-runtime` workflow job runs on both `ubuntu-24.04` and `ubuntu-24.04-arm`.
The required `xiaoge-memory` job runs Miri and
`check_xiaoge_native_memory.py`, including complete external-library instrumentation.
Native features are explicit so portable workspace/musl checks retain their
existing scope. Both new jobs feed `rust checks`; skipped/cancelled/failed
results are rejected. Local CI-policy tests and shell syntax checks pass.
Independent review found that the original Miri-to-`tee` pipeline could hide a
failed Cargo command. The exact workflow step now uses `set -euo pipefail`;
a retained failing-before/passing-after test verifies that exit 42 stops the
first mode and remains the job's exit status. The reviewer reported no additional
concrete ownership or HTTP/IPC defect in the inspected paths.

Hosted exact-head results are still pending. ARM emulation is functional
evidence, not device IPC timing or a performance result. Normal startup,
complete daemon composition, existing log upload and the user's first device
comparison remain full-runtime acceptance gates. This component does not
establish CPU savings or a complete Rust runtime.
