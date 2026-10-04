# Native USB/AMD runtime inventory (#154)

Issue: https://github.com/bin9208/openpilot-rust/issues/154
Parent runtime gate: https://github.com/bin9208/openpilot-rust/issues/1
UI consumer: https://github.com/bin9208/openpilot-rust/issues/148
Baseline: `3eac955988b0f4cd616d86b2873078ae4d15787f`.

This inventory was recorded before implementation. Full responsibility coverage
below remains pending; the intermediate checkpoint records completed host checks.
The existing CPU/QCOM runtime does not implement USB+AMD. The preimplementation
comparison contract is `rust/crates/usbgpu/tests/contract.json`.

| Responsibility | Original source | Required native evidence |
| --- | --- | --- |
| USB discovery, firmware, speed, power and link health | `system/hardware/usbgpu.py` | sysfs and libusb bytes, diagnostics, fresh-process retry/timeout/cleanup |
| Device selection, current/previous compiled paths, discovery grace | `modeld/helpers.py` | unchanged-source state/path traces and owned files |
| Model download/update and active/previous state | `modeld/big_model.py`, `big_model_status.py` | owned HTTP resume/hash/size failures and atomic Params/file transitions |
| Verified precompiled install and rejection | `modeld/precompiled_model.py`, `precompiled_validation.py` | catalog/archive/receipt decisions, persistent transient-versus-permanent failures |
| Native compile and artifact import | `modeld/compile_modeld.py`, pinned `compile_onnx.py`/`compile_warp.py`, `common/file_chunker.py` | source kernel/artifact/metadata identity, complete import and supported compilation |
| Generic and split graph execution | `precompiled_runner.py`, `precompiled_worker.py`, `generic_model_runtime.py` | native process, tensor layouts/aliases, recurrence, pipe/deadline/cleanup behavior |
| C3 QCOM pre-upload and C4 AMD warp | `local_gpu_warp.py` and artifact warp implementation | original pixels and exact per-mismatch boundary acceptance, fallback and transfer layout |
| Model daemon startup/live fallback | `modeld.py` | discovery/load retry/timeout, Params, diagnostics/tmux policy, same-frame warm internal fallback |
| USB/PCIe bridge and interprocess exclusion | `runtime/support/usb.py`, `runtime/support/system.py`, `common/usbgpu_bus_lock.py` | actual native libusb fixture, exact command bytes/order, partial/retry/error cases |
| AMD firmware/init, MMU, queues and synchronization | `runtime/ops_amd.py`, `runtime/support/am`, `runtime/support/hcq*` | source register/ELF/argument/dispatch traces, ownership/teardown/sanitizers |

The source default is Cinque v3 checkpoint
`b9facbcc-4d47-410e-b3ce-dfcbad12ba92/56320/f78ed37d-afad-4dbc-8050-40ea885eedde/12864`,
PKL size 776,634,338 and SHA-256
`e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b`.
An existing local copy matches that source hash. Its observed cached companion
archive is 2,731,184 bytes, SHA-256
`a25b81e90d5259c27bc49b3a93ab499c1909a33e133cba974cf023cd94727eeb`.
This local observation does not independently authenticate a remote catalog.
No NAS download was performed. The companion uses `_TinyJit`/`CapturedJit` and
`hcq2` UOp graphs, whereas the repository's runtime uses the older HCQ layer;
both source families must stay explicit. Read-only inert pickle inspection
records the actual model's seven input tensors and four outputs, including
three in-place recurrent state pairs.

UI boundary agreed with #148: sibling executable `openpilot-usbgpu-check`,
optional `--timeout-seconds 15` and `--allow-link-errors`. Default clean-link
checking remains enabled. One JSON object `{ "error": null | string }` and exit
zero means a completed source diagnostic, including normal failure strings.
Invalid arguments or inability to spawn/communicate with its native probe use
nonzero exit and stderr. Timeout is per attempt; two PCIe-not-ready attempts
with the one-second retry delay can take 31 seconds plus process overhead.
The UI owns presentation; this backend owns the actual power/GPU probe.

External boundaries may retain native libusb, Linux drivers, GPU firmware,
original model/kernel artifacts and native compiler/math libraries with recorded
versions/provenance. A Python tinygrad child or a helper that only reports status
is not completion. Physical GPU/device results, performance and complete normal
startup/upload acceptance remain separate. No device-test request is authorized
for this component.

## Intermediate foundation checkpoint (2026-10-01)

The library now supplies native sysfs/power/status parsing, the owned diagnostic
child wrapper, synchronous UI model availability, the libusb CXX ownership
boundary, USB3 BOT/UAS command construction and custom ASM vendor commands.
The native GPU probe executable and AMD/model runtime are **not implemented** by
this checkpoint; the diagnostic wrapper requires a real probe and does not
substitute a success stub. No production daemon selection changes here.

Host evidence is under `.omo/evidence/usbgpu-154/` in the issue worktree:

| Scenario and invocation | Binary observable | Captured artifact |
| --- | --- | --- |
| `python3 rust/tools/check_usbgpu_hardware.py <target>/debug hardware-output` | 452 unchanged-source discovery/status/power comparisons; 10 source/native child cases; cancellation/reap and missing child checked | `hardware/hardware.json`, `hardware/process-checks.json`, `hardware/cancellation.json` |
| `python3 rust/tools/check_usbgpu_asm.py --binary <target>/debug/examples/asm_trace --evidence <file>` | 41 exact source command/result/sleep comparisons, including startup recovery, retries, errors, SRAM padding and cache | `asm-traces.json` |
| `python3 rust/tools/check_usbgpu_status.py --binary <target>/debug/examples/model_status --evidence <file>` | 192 active-model status comparisons; previous selection never marks the active model compiled | `status-parity.json` |
| `usb_boundary_fixture` with local `LD_LIBRARY_PATH` and each recorded `USB_FIXTURE_MODE` | Eight actual CXX/dlopen fixture scenarios exit zero, cancel/drain before freeing, and release streams/interface/handle/context in order | `usb-boundary/results.json` |
| `USBGPU_SANITIZE=1 cargo build ... --example usb_boundary_fixture`; same eight invocations with ASan/UBSan enabled | Exit zero with clean sanitizer stderr; instrumented binary preserved | `usb-boundary/sanitizer-results.json`, `usb-boundary/usb_boundary_fixture-sanitized` |
| `cargo test --manifest-path rust/Cargo.toml -p openpilot-usbgpu` | Nested thread/process exclusion and bulk zero-byte-only retry cases pass alongside discovery/model tests | `foundation-tests.log` |
| `MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml -p openpilot-usbgpu --no-default-features --test discovery` | Two discovery/power tests pass; native FFI is excluded | `miri-discovery.log` |
| `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-usbgpu --all-targets -- -D warnings` | Exit zero | `clippy.log` |

These scenarios establish only the named host behavior. UAS full-window
source behavior, stock ASM, PCIe BAR/MMU, AMD initialization/queues, native
artifact execution, provisioning/compiler/warp and driving integration remain
in progress. Device and CPU/performance claims remain unsupported.
