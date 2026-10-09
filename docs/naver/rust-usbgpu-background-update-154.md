# Native optional-model update command (#154)

`openpilot-usbgpu-model --ensure-if-egpu` now implements the generic installed
model path of `openpilot/selfdrive/modeld/big_model.py::main`. It reuses native
manifest/download/state/status and companion readiness. The command never runs
the downloaded Python runtime or opens a GPU.

Before reporting status, it evaluates SuperSpeed presence, native compiled
readiness and the real `UsbGpuHardwareSeen` Params key. Known hardware remains
remembered while unplugged. Only Params I/O errors have the source C++ bool/put
behavior; invalid key/prefix errors remain failures. Params construction and
state-file I/O failures remain outside the delivery exception boundary.

If eligible, the command reports checking, optionally waits for the manifest
host only when no usable active model exists, then composes download/verifying
and compiled/ready reporting. Network readiness preserves the original
per-address connection bound `max(0.1, min(2, remaining))`, two-second retry
interval capped by remaining time, and caller-selected outer wait. Download
callbacks are fallible: status-write failure stops delivery rather than continuing
and installing a model. Ordinary delivery errors report error and return zero,
retaining active/previous selection so optional delivery does not stop internal
startup. If the error status itself cannot be written, the command fails.

The updater retains default process TERM behavior. Boot/smoke commands retain
their existing cooperative cancellation handlers and descendant ownership.

## Observed controls

Private artifacts are under the #154 worktree's
`.omo/evidence/154-runtime-resume/`; invocation files retain commands, environments,
source Cython binding path, outputs and exact executable identity.

`background-update-green-v2` contains 15 original/native observations using actual
loopback TLS requests, owned files and the unchanged Cython Params binding:

- Unknown hardware and USB2 skip; present, remembered and genuinely compiled
  native packages permit delivery.
- History read/write I/O behavior matches the source C++ recipient.
- Existing active state skips the optional network wait. Failed manifest and
  network checks preserve that state.
- Complete delivery selects the verified file. A fixed-length response declaring
  20 bytes but closing after 3 retains the same 3-byte `.part` and fails final
  size verification without installation. Truncated chunked framing retains
  its existing error behavior and does not install.
- Status, download-progress status, Params constructor and pre-reporter state-read
  failures have explicit nonzero-exit/cause assertions. Both sides preserve the
  original scope of those failures.

The comparison includes return outcomes, requests, persistent fields, file sizes
and partial hashes. Wall-clock timestamps and general diagnostic wording are
excluded; raw diagnostics remain captured. In particular the blocked Params
root reports source `RuntimeError`/errno 20 and native I/O errno 17. They are
separately asserted failures, not asserted equal diagnostics.

`background-update-v1` preserves the lost-prefix RED. The actual reader trace in
`background-prefix-trace/result.json` shows 3 bytes followed by `UnexpectedEof`;
the caller's accumulation loop previously returned before writing those bytes.
The repair treats fixed Content-Length EOF as a short read before unchanged
size/hash rejection. Transfer-Encoding and other read errors stay propagated;
no HTTP provider or vendor patch was needed. `background-update-v2` additionally
preserves the pre-reporter state-read RED. The first attempted strict constructor
assertion guessed source errno 17; the recorded source reports 20, and the
corrected full comparison is the final green receipt.

`background-cancel-red` preserves ignored native TERM. In
`background-cancel-green`, both actual processes reach the printed 60-second
network-wait phase, then exit by TERM with no forced kill. The reserved loopback
port remains owned during this control. `background-manual/result.json` records
actual `--help` and negative-wait rejection with exit 2.

`background-update-build/` contains bounded selected builds, all-target strict
Clippy, formatting/diff and helper lint/syntax/rule checks. The only lock delta is
the existing `openpilot-params` dependency edge; no package versions or downloaded
dependencies changed. Earlier numeric, USB, warp and unaffected download/boot
controls are reused.

## Remaining integration

The 15-case matrix covers the current generic active/precompiled path. Original
`helpers.usbgpu_compiled_path` also admits a previous ONNX-derived local artifact
unless the current model is PKL-only. That historical/local selection remains
pending genuine `run_policy` / `comma-run-model` exporters and queue adapters;
the updater must use that shared selection when implemented. Fake chunk manifests
remain insufficient for native readiness and cannot replace the historical
positive selection assertions.

Normal launcher composition still needs the source background duplicate guard,
`/tmp/big_model_update.lock`, nice 10 / optional ionice class 3, append logging and
non-blocking manager startup. The command alone is not that startup handoff or a
complete runtime/device candidate. Native package/AGNOS dependency validation and
the user's first device comparison remain separate gates. Explicit build-time
Python conversion remains permitted; installed execution must consume verified
native companions and queues.
