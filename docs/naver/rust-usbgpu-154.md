# Native USB/AMD runtime inventory (#154)

Issue: https://github.com/bin9208/openpilot-rust/issues/154
Parent runtime gate: https://github.com/bin9208/openpilot-rust/issues/1
UI consumer: https://github.com/bin9208/openpilot-rust/issues/148
Baseline: `3eac955988b0f4cd616d86b2873078ae4d15787f`.

This inventory was recorded before implementation. Full responsibility coverage
below remains pending; the intermediate checkpoint records completed host checks.
The existing CPU/QCOM runtime does not implement USB+AMD. The responsibility table below is the preimplementation comparison contract.

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


## Dev integration foundation (2026-10-08)

The candidate based on `210bfddc` transfers only the existing usbgpu crate and
four source-contract helpers from preserved integration `1dd955e1` (feature
history `ad84df44`/`52e8e354`). The MIT/LGPL notices and declaration-only libusb
header are preserved. No UI application or Xiaoge runtime is included, and no
GPU implementation, option or production process selection is added.

The existing workspace checks and generic ARM workspace build include this
crate. Its three original hardware/status/ASM comparison commands are connected
to the workspace job with retained evidence; there is no new required job.
The native USB boundary still includes the existing project-owned CXX adapter.
Status parsing and a diagnostic child wrapper do not implement the native GPU
probe, AMD/MMU/queue runtime, model compile/import/execution, warp or live fallback.
These gaps remain explicit in the library inventory; the wrapper requires a
real probe and retains a distinct spawn failure when that probe is absent.

Earlier host foundation and owned-boundary results above are reused historical
evidence. Only offline metadata, formatting, YAML/shell and small Python static
checks are performed here. New candidate builds/Actions and actual ARM/USB/GPU
execution are pending. No local native build, install, corpus rerun or device
access occurs. Full startup/upload and device/performance acceptance are separate.
Host-filtered locked offline metadata passed; full-target offline resolution is
limited by the uncached Redox-only `redox_syscall 0.5.18` archive. Its existing
lock entry is preserved and hosted Cargo resolution remains pending.

## Preserved implementation resumed (2026-10-09)

The following historical bridge/ASIC sections were recovered from the preserved
`5283090a7` issue worktree. The original raw `.omo/evidence/usbgpu-154` receipts
and old binaries were removed during user-authorized cleanup. Their recorded
results describe prior bounded work, not current executable acceptance evidence.
The preserved source additionally contains HCQ2, native probe/worker/client,
QCOM warp and driving-model integration drafts. Full pinned-model numerical
execution and current source-format/fallback/lifecycle checks remain unfinished.
The 2026-10-08 dev integration record above is retained unchanged.

## Bridge, GPU memory and firmware checkpoint

Stock ASM command policy now has 49 comparisons against the unchanged controller
at its SCSI boundary. Native UAS windows intentionally correct inherited stale
results after 31 commands; the reproduction, consumer audit and compatibility
exception are tracked separately in [#160](rust-usbgpu-uas-160.md).

Further host evidence under `.omo/evidence/usbgpu-154/`:

| Scenario / runner in `rust/tools/` | Binary observable | Artifact |
| --- | --- | --- |
| `check_usbgpu_stock_asm.py --binary <target>/debug/examples/stock_asm_trace --evidence <file>` | 49 command/result comparisons, including multi-window reads, caching, PCIe and SRAM sizes | `stock-asm-traces.json` |
| `check_usbgpu_pci.py --binary <target>/debug/examples/pci_trace --evidence <file>` | 16 exact config/BAR transaction traces across BAR sizes/types and resizable BAR capabilities | `pci-traces.json` |
| `check_usbgpu_allocator.py --binary <target>/debug/examples/allocator_trace --evidence <file>` | 7,130 allocation/free addresses match across 20 seeded scenarios | `allocator-traces.json` |
| `check_usbgpu_page_table.py --binary <target>/debug/examples/page_table_trace --evidence <file>` | 2,098 page/flag/fragment results match | `page-table-traces.json` |
| `check_usbgpu_memory.py --binary <target>/debug/examples/memory_trace --evidence <file>` | 12 complete allocation/mapping/free write traces match across gfx9/10/12, table reserves and GMMU states | `memory-traces.json` |
| `check_usbgpu_discovery.py --binary <target>/debug/examples/discovery_trace --evidence <file>` | Eight discovery and complete register-binding results match, including 32/64-bit bases and multiple instances | `discovery-traces.json` |
| `check_usbgpu_firmware.py --binary <target>/debug/examples/firmware_trace --firmware <verified-directory> --evidence <file>` | Eight pinned blobs yield exact SOS/SMU/12 descriptor bytes and microcode entry addresses | `firmware-traces.json`, `firmware-fetch.json` |
| `cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml -p openpilot-usbgpu --no-default-features --test memory` | Three allocation, mapping/reuse and aperture tests pass | `miri-memory.log` |

`generate_usbgpu_amd_metadata.py --check` verifies the checked-in native register,
constant and C-layout data against source hashes. It parses declarative AST data
without importing the source modules. Production code reads the resulting JSON;
it does not execute Python. The 28 register modules and 105 layouts retain their
original tinygrad/AMD source provenance through the generated source-hash table.

The local system supplied four matching compressed firmware files. Four others
were obtained from the original pinned linux-firmware GitLab commit and verified
against the original hash table. These external blobs stay in the private
fixture cache; no firmware or vehicle installation was performed. Native bus
binding exists, but hardware initialization, command queues, model import/
execution, provisioning, compilation, warp and daemon integration remain pending.

## ASIC initialization and queue setup checkpoint

Native Rust now implements the inherited GPU discovery-to-finalization sequence:
GMC page tables/hubs, PSP firmware commands, SMU clocks/power, GFX and SDMA
initialization, interrupts/recovery, and compute/copy ring setup. Production
uses the native bridge bus; Python remains an offline source oracle only.

`rust/tools/check_usbgpu_asic.py --binary <asic_rpc> --firmware <verified-directory>
--evidence <directory>` compares both implementations against the same owned
gfx1200 hardware model. Eight cases pass: cold/warm/dirty startup with default
and 42.5 W limits, PSP response failure, and cold startup with both queue rings.
All native events match in order (325–1,024 events per case); extra source PTE
rereads may be skipped only after auditing their values against prior owned
writes/zeroes. Register, firmware, queue descriptor, sleep and final state
observations remain compared. The fixture supplies hardware DPM frequencies;
it does not replace source clock policy.

The queue comparison caught an eager native leaf-table read absent in the
source. Inspection/free now short-circuit leaf nodes as the source does. The
preserved old binary still fails exactly that queue case with the same checker.

Evidence: `.omo/evidence/usbgpu-154/asic-manifest.json` links raw traces, exact
invocations, binary hashes/preserved binaries, 13 passing crate tests, strict
Clippy, three strict-provenance Miri memory tests, 12 mapping/free comparisons
and the failing old-binary control. Two initial Miri infrastructure failures
(stale dependency metadata and competing toolchain sysroot setup) remain in
separate logs; a fresh target with the pinned toolchain passes.

These are host hardware-boundary comparisons, not physical GPU execution or
vehicle validation. Command submission, ELF programs, native model import/
execution, provisioning/compilation, warp and daemon integration remain pending.

## Current owned-host checkpoint (2026-10-09)

The resumed dev-based worktree preserves the sources above and replaces the
project-owned CXX USB adapter with Rust ownership and checked libusb calls.
Current worker/client, native warm fallback, emulated probe, dispatcher, USB
failure/lifetime and memory-check results are recorded in
[the resumed runtime checkpoint](../rust-port/usbgpu-native-runtime.md#october-9-resumed-checkpoint).
Its `.omo/evidence/154-runtime-resume/checkpoint/receipt.json` identifies the
frozen files, eight binaries, command receipts and failures.

Full-model numerical acceptance remains failed/incomplete: 1,149 native outputs
are NaN and the original replay timed out. Those failures are explicit in the
linked record and [issue update](https://github.com/bin9208/openpilot-rust/issues/154#issuecomment-6080676637).
The passing fallback and USB controls do not replace this gate. Model assets,
provisioning/compiler integration, AMD/QCOM warp, source/recurrent equivalence,
exact-head CI and normal startup/log-upload composition remain required. No
device/NAS access or deployment occurred.
