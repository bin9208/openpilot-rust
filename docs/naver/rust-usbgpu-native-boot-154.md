# Native precompiled USB/AMD boot preparation

Issue: [#154](https://github.com/bin9208/openpilot-rust/issues/154).
Sources: `openpilot/system/manager/build.py::build_usbgpu_model` and
`openpilot/selfdrive/modeld/{precompiled_runner,precompiled_validation}.py`.

The new `openpilot-usbgpu-model` command exposes boot preparation, smoke execution,
readiness and active-artifact queries. Boot reuses the verified native companion
package described in `rust-usbgpu-native-provisioning-154.md`. It never imports the
downloaded Python runtime or compiles warp kernels at runtime.

For an installed generic PKL, boot preserves the original SuperSpeed presence
gate, per-device camera selection, five zero-image smoke inputs, validation-cache
reuse, transient waiting policy and fatal rejection/internal fallback. Presence
reads only IDs and floating-point speed; unrelated USB description failures do
not reject the artifact. Missing identity disables cache reuse and tests both
camera sizes. Cache-write I/O failures remain best effort. Enumeration, validation,
final compiled-status and output errors share the boot failure receipt boundary.
Progress callback errors abort installation immediately, retaining its partial
file and leaving selection unchanged.

Validation cache schema 2 identifies the actual native worker/runner binaries,
asset manifest, installed catalog, device, OS and camera sizes. It deliberately
replaces Python/NumPy/source-module identity. Cache keys are provider diagnostics,
not claimed identical to Python keys.

The smoke client retains the original 110-second load deadline, 20-second first
inference deadline and one-second later deadlines. Boot's outer validation bound
remains 300 seconds. Native cleanup uses an owned process group: TERM allows the
runner to cancel and reap its worker, with a two-second fallback before KILL.
This strengthens descendant cleanup compared with the original direct subprocess
timeout. The leader is observed with `waitid(WNOWAIT)` and remains unreaped until
the final group signal, preventing numeric PGID reuse during cleanup. Cancellation
does not blacklist a healthy artifact.

## Actual controls

All paths below are relative to the owned worktree's ignored
`.omo/evidence/154-runtime-resume/`. Invocation files contain commands, environment,
raw outputs and exit status. These are host controls using real processes/files
and an owned deterministic protocol runtime; they do not execute a physical GPU.

| Scenario | Binary observable | Artifact |
| --- | --- | --- |
| Inactive model | Both original function and native CLI return false without worker admission | `boot-inactive-v1/result.json` |
| USB2, present, cache reuse, missing identity, cache-save error, compiled-status error, transient/fatal worker failures | Eight source observations match eight native callers on the corrected-presence executable; five packed input hashes per selected camera, phases/receipts/rejection and return values agree | `boot-composition-current/{result,invocation}.json` |
| Status-write error acceptance | Requires actual original `IsADirectoryError`, native exit 1 and errno 21; all other ordinary cases require native exit 0 | `boot-error-assertion-strengthening.json` |
| Irrelevant invalid manufacturer | Earlier native rejection is retained; corrected actual boot pair succeeds | `boot-presence-red/result.json`, `boot-presence-green/result.json` |
| IDs/speed/read boundaries | Sixteen original/native observations agree, including float/exponent/underscore speed, NaN/Inf, missing/invalid relevant fields and USB2 | `boot-presence-fields-v1/result.json` |
| Progress status-write failure | Actual callback error stops after 1 MiB, preserving the same partial bytes/hash and requests with no installed marker | `boot-progress-v1/result.json` |
| Cancellation | Actual boot runner/worker PID and start-time identities, readable pidfds, reaped processes and removed shared file; no failure/rejection receipt | `boot-leader-cancel/result.json` |
| Leader ownership and noncooperative runner/worker | Actual zombie leader remains unreaped until final signal; production wait helper with a test-only 50 ms bound kills and reaps both TERM-ignoring children after the bounded fallback | `boot-leader-ownership/tests.json` |
| Corrected normal-exit caller | One actual native boot caller matches the retained source present-model observation after the unreaped-leader correction | `boot-leader-normal/result.json` |
| Closed stdout | Original/native persist transient boot failure and waiting status; native reports errno 32 with exit 1 rather than panicking | `boot-closed-output/{source,result}.json` |
| Required checks | Selected builds, all-target strict Clippy, fmt/diff, six Python Ruff/format/syntax/rule checks | `boot-final-gate/` |

The first draft boot executable was not recorded by hash before replacement.
Its paired observations remain explicitly draft evidence in `boot-composition-v2`.
The native-only rebind in `boot-composition-current` reuses those source captures
and records the corrected executable SHA before and after. The later CLI output
change uses fallible writes; its affected closed-output control is separate.
These are successive executable identities, not eight repetitions on the latest
binary: corrected-presence rebind `db2c…`, fallible-output control `36c6…`, then
unreaped-leader normal/cancel controls `1db1…`. Each full SHA is recorded in its
invocation/result. Earlier unaffected observations are reused at the later step.
The closed-output control subsequently changes the owned USB2 fixtures' status
files; earlier captured JSON observations remain unchanged. Source fixture import
binding and typed declaration/style failures are retained and qualified.

## Remaining runtime/package work

This checkpoint does not complete #154 or the first-device-test gate. Native
background download/remembered-hardware policy and normal-startup orchestration
remain to be wired. The current real worker resolves companions/firmware from
its executable sibling or legacy adjacent files. The default package layout is
compatible; explicit boot asset roots still need propagation and validation at
that real consumer boundary. The protocol fixture does not prove that load path.
The local `run_policy` and `comma-run-model` queue contracts
still need genuine native companion exporters/adapters. Their historical positive
driving-selection assertions remain required; synthetic ONNX/chunk markers cannot
stand in for a native provider. Explicit Python build-time conversion remains
permitted by the approved design, while installed inference stays native.

The earlier 473+473 finite, byte-identical host proof is documented in
`rust-usbgpu-emulator-alignment-260.md` and is reused unchanged. Device package
acceptance, USB/AMD hardware execution, AGNOS dependencies and driving acceptance
remain separate pending gates.
