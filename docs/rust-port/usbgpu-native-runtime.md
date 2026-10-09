# Native USB AMD runtime checkpoint

Issue: [#154](https://github.com/bin9208/openpilot-rust/issues/154), within the
full runtime conversion [#1](https://github.com/bin9208/openpilot-rust/issues/1).
This checkpoint is intermediate engineering evidence, not a device handoff.

The Rust `usbgpu` crate now owns AMD ELF/descriptor loading, compute and SDMA
packet encoding, ring submission, mapped buffers, scratch allocation, signals,
upload/download, and synchronized program release. The existing hardware,
firmware and transport layers remain its dependencies. Firmware and compiled
model kernels remain external artifacts; no Python process implements native
runtime execution.

## Pinned HCQ2 artifact

The Cinque v3 model is SHA-256
`e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b`.
Its runtime archive is SHA-256
`a25b81e90d5259c27bc49b3a93ab499c1909a33e133cba974cf023cd94727eeb`;
the archive pin in [the model inventory](../cinque_v3_model.md) names tinygrad
`d5e17c935daf11f6318e45aade9528f71b8fbdcc`. The extracted AMD, HCQ2 and USB
source files were compared directly with the archive. A parent checkout's Git
HEAD is not provenance for that extracted archive.

`rust/tools/import_usbgpu_hcq.py` is a build-time, inert-record importer. It
does not import pickle globals. It reads the existing model through `mmap` and
emits a descriptor referring to original file ranges: 585 serialized buffers,
148 placeholders, 3,166 byte/address patches, and a 1,221-operation host
dispatcher for 473 GPU kernel invocations. The 792 KiB descriptor does not
duplicate the 776 MB model. The runtime verifies the original model hash and
individual blob hashes before using their data.

`hcq_vm` executes the host dispatcher's integer, pointer, loop and USB-call
operations in owned, range-checked memory. `hcq_model` binds those operations
to buffers and preserves the three next-state output aliases. `hcq_gpu` binds
the model to the native GPU layer and the pinned model's 1 MiB compute ring.
The current pinned-model backend requires gfx1200 and the custom bridge.

The loader reuses the validated SDMA uploader for buffers without CPU access.
It is not claimed to have the pinned HCQ2 loader's exact initialization I/O
sequence. Final buffer contents, patches, synchronization and lifetime behavior
must be evaluated separately from transfer-sequence equivalence.

## Observed host evidence

The rows below are historical October 1–2 observations. Their old private
evidence and retained executables were removed during the user-authorized cache
cleanup. They are not current acceptance receipts; the resumed checks follow
this section.

All evidence below is under the private worktree's
`.omo/evidence/usbgpu-154/`; the model and captures are not Git artifacts.

| Scenario | Invocation/tool | Observable | Receipt |
| --- | --- | --- | --- |
| Original/native GPU initialization, custom/stock, AQL/PM4, SDMA enabled/disabled | `check_usbgpu_runtime.py` | 8/8 source comparisons pass | `resume-gpu-init-green/comparison.json` |
| gfx1200 affine ELF, upload, dispatch, signal, download, program release, ASIC finish | `usbgpu_compute_fixture.py --kind source/native` | 32 f32 outputs exactly match source and mathematical result; timeline 5 | `compute-comparison-final.json` |
| Pinned embedded C dispatcher versus native VM at ordinary/wrapping ring positions | `check_usbgpu_hcq_dispatch.py` | 61 transfer calls and all 53 comparable host-buffer hashes match in each scenario | `hcq-dispatch-final/comparison.json` |
| Native control/bulk success, negative and short transfers using an owned libusb shared library | `check_usbgpu_hcq_transfers.py` | six cases pass; failures stop at the first call; handles/interfaces/context released | `hcq-transfer/results/comparison.json` |
| Owned model file and recurrent state alias | `cargo test -p openpilot-usbgpu --test hcq_model` | changed artifact rejected; next-state output observes the same allocation | `hcq-model-first.log` |

The affine source makes 1,759 boundary calls and Rust makes 1,473. The difference
is 286 source page-table rereads; the remaining ordered I/O is identical. Those
omitted reads are an explicit difference, not raw-trace equality. The owned GPU
emulator implements the source mock's missing SDMA WRITE packet and adapts its
obsolete `DEFINE_VAR` parser check to the current PARAM representation. Its
Clang half-precision compiler failure is retained as a failed run; the full-model
fixture uses the source LLVM CPU renderer. These are fixture boundaries, not
physical GPU validation.

The pinned C dispatcher continues 60 dependent calls after a negative or short
first control transfer. Native execution deliberately fails closed on negative
or short transfers. This differs from the source failure behavior, preserves
successful transfer fields and bytes, and prevents dependent commands after a
failed command. The independent UAS stale-result defect
[#160](https://github.com/bin9208/openpilot-rust/issues/160) remains separate.

## October 9 resumed checkpoint

The source was transferred to the current `dev`-based issue worktree without
replacing the workspace or inherited hardware-check timeout fix. Current raw
command outputs and results are in `.omo/evidence/154-runtime-resume/` there.
The pinned model and firmware are reused in place.

The USB transport now calls the native libusb ABI directly from Rust. Its
single-thread owner retains context, handle, transfer buffers, boxed callback
state and library until every submitted terminal callback has run. The former
project-owned C++ wrapper and build edge are removed; libusb and its LGPL SDK
header remain native external dependencies. The 124-transfer window, interrupted
event handling, and cancel/drain behavior are preserved.

| Scenario | Current observable | Receipt |
| --- | --- | --- |
| Original client and owned native worker | Two camera sizes, four frames, exact layout/input hashes, loader-thread and failure cleanup | `worker-protocol/comparison.json`, `worker-lifecycle/comparison.json` |
| Rust client and owned native worker | Both sizes: normal, disconnect, nonfinite output and timeout controls | `rust-client-*-invocation-v2.json` |
| Native selection and loaded internal fallback | Eight flag/lifetime cases; same-frame fallback bytes match the direct native CPU baseline for both sizes | `native-selection-invocation.json`, `native-warm-fallback/comparison.json` |
| Source random probe and emulator execution | Two actual kernels produce 4,194,304 byte-identical output bytes with 128 download chunks | `probe-numeric/comparison.json` |
| Artifact C dispatcher and Rust VM | Successful control/bulk fields and host buffers agree at ordinary and wrapping ring positions | `hcq-dispatcher/comparison.json` |
| Rust USB foreign-library boundary | Eight old-wrapper event traces match; 124 requests finish and 125 are rejected; instrumented owner/library controls are ASAN-clean | `usb-final-gates/comparison.json`, `usb-asan-link-export/comparison.json` |
| Production callback memory access | One actual callback test passes four Miri levels, including strict provenance and Tree Borrows | `usb-miri/level-1.json` through `level-4.json` |

Miri covers the Rust-side callback test, not foreign libusb execution. ASAN
covers Rust plus the instrumented owned libusb fixture, not physical USB or the
system library. Its first run could not load the instrumented fixture because
sanitizer symbols were not exported; the selected linker export corrected that
test configuration without changing production code.

Full model acceptance remains **failed/incomplete**: the native emulator pass
executed all 473 kernels, but 1,149 of 18,452 outputs were NaN (plan, lead,
lead probability, desire state and action). The original C dispatcher replay
reached its 467th kernel before the 20-minute harness bound expired. Output and
recurrent-state parity are not established. `model-numeric-v1-partial.json` and
`model-numeric-v1-nonfinite.json` retain both failures. The next diagnostic
localizes the first active floating-point NaN without accepting transient or
unused register values as the cause.

## Remaining delivery gate

Full pinned-model numerical execution and source comparison are still being
validated. The first owned run loaded and linked the entire artifact and reached
shader execution, where the source emulator exposed the compatibility issues
listed above. Neither that progress nor the affine test is full-model inference
acceptance. Native offroad-probe and driving-worker integration remain in scope.

Normal startup, log production and the existing upload path must work across the
entire project-owned Rust runtime before the user performs the first C3X
comparison. No device/NAS access, deployment, production daemon selection,
physical GPU result or CPU-savings claim is part of this checkpoint.
