# Native worker asset binding (#154)

The native boot runner and driving-model caller now pass the validated asset
root and its manifest SHA-256 to each worker child. This replaces the previous
worker fallback to unchecked adjacent HCQ descriptors or firmware directories.
Existing `Launch`, worker pipe, shared-memory layout and inference commands remain
compatible. No model instructions, scheduling or numerical thresholds changed.

`worker_artifact::bind` validates the catalog and the required native package.
The real worker calls the same package preparation path before opening a GPU;
it checks the expected manifest identity, the selected PKL's complete hash and
size, checkpoint, metadata, descriptor and both camera companion closures.
Its firmware and AMD/QCOM warp paths come from that validated root. The validation
cache key uses the captured manifest identity passed to the runner and child.
The manifest provides trusted-package integrity, not an independent signature.
Packaged assets must remain immutable during use.

## Actual focused controls

Receipts are private under `.omo/evidence/154-runtime-resume/` in the #154
worktree. Each invocation records its command, environment and binary SHA.

| Scenario | Observable and captured artifact |
| --- | --- |
| Real worker, both selected camera sizes | `--check-artifacts` returns the actual descriptor, warp, checkpoint, output layout and validated root; `worker-assets-v2/result.json` |
| Missing root, corrupt warp, wrong manifest identity, wrong checkpoint, corrupt PKL | Normal worker mode exits 1 with the exact expected pipe/stderr error and no owned libusb initialization; same nine-case receipt |
| Adjacent unchecked descriptors/firmware | A valid explicit package ignores poisoned adjacent files; a missing default package rejects despite those files; same receipt |
| Original corrupted-PKL worker | The unchanged Python worker exits 1 with `ValueError: precompiled PKL checksum mismatch` before runtime imports; `worker-source-corrupt-v1/result.json` |
| Actual client overrides wrong ambient root | `Client::launch_with_assets` reaches the real worker's owned libusb open-error boundary; exact `init`, list release and clean context exit; `worker-binding-v1/result.json` |
| Package changes after binding | The actual client/worker rejects the changed manifest before libusb initialization; same receipt |
| Boot runner child binding and cache | An owned protocol worker observes the root/SHA, an unchanged second boot launches no worker, and a changed manifest invalidates the key and relaunches; same receipt |
| Bound boot cancellation | Actual runner/worker PID/start-time and pidfds establish both reaped, shared file removed and no artifact rejection; `worker-binding-cancel/result.json` |

The nine-case worker uses the owned libusb fixture with an explicit open-error
mode as a defensive fallback. The successful structural cases do not open a GPU.
The client success boundary reaches that owned foreign provider and deliberately
fails opening it. Boot cache and cancellation use the unchanged owned protocol
worker; they do not establish physical GPU readiness or numerical execution.

Final gates are `openpilot-usbgpu` all-target strict Clippy, the
`openpilot-driving-modeld` library strict gate, package formatting, diff checks,
and Ruff/format/syntax plus the optional Python rule check for both new drivers.
They are captured in `worker-assets-final-gate/`. The previously accepted full
473+473 host numerical proof, USB callback/drain proof and warp comparisons are
reused because their execution paths and arithmetic are unchanged.

## Remaining runtime acceptance

This closes the explicit generic installed-worker root/identity connection.
Native optional-model background/startup orchestration and genuine local
`run_policy` / `comma-run-model` exporters and queue adapters remain unfinished.
The historical `usbgpu_selection_trace` fixture's fake ONNX/chunkmanifest cannot
exercise its positive branches under real native readiness; its existing
positive assertions remain required with a genuine local provider. Target SDK
packaging, normal full-project startup and the user's first device comparison
are separate remaining gates. Build-time Python conversion is permitted by the
approved design; downloaded Python runtime queues or warp compilation are not.
