# Rust radard conversion (#207)

Issue: https://github.com/bin9208/openpilot-rust/issues/207

The candidate ports the original `radard_dpath` process, controller, trajectory
prediction, primary and cutout selection, cached paths and continuous
`radarState` publication. It reuses planner lead dynamics and RadarCAN policies.
Original MIT/PSF provenance is retained with the source. Production daemon
selection and the NAS replay service are unchanged.

Independent host evidence is preserved in the original issue worktree under
`.omo/evidence/radard-207-resume/summary.md`. Source recorders compare 4,974 calls
across 235 owners with exact outputs, state and cached paths. Real Params/msgq
comparison covers 200 normal frames across Hyundai modes, vision/corners, MEB
and unavailable Toyota, plus 40 frames for missing inputs, paused model input,
invalidity and recovery. Nine Rust tests, all-target Clippy and formatting pass.

Waiting SIGINT/SIGTERM, malformed CarParams and running SIGTERM have the same
observed source/native exit classes. Invalid numeric Params remain an explicit
fatal-boundary difference: original C++ Params aborts with -6; the typed Rust
error exits 1. No abort is added merely to imitate the original crash class.

The functional executable SHA-256 is
`a08ca9f9975c549156dd232c05d80e33415196116e9f5364cd8a5eeca9811aff`.
The final formatting-only executable is
`6b24a1ca77521d1fde50cca26598c4bbf6953efd2598eb717ee0ea7611523cac`.
These identify independent worker results, not a new integration execution.

The daemon requires Linux facilities, Params/CarParams, original cereal/msgq
inputs and the external ZeroMQ diagnostic library. Build with
`cargo build -p openpilot-radard --locked`; its default feature selects the
native executable. The reusable `rust/tools/radard_ipc.py` and
`radard_lifecycle.py` accept `--python`, `--binary`, `--binding` and `--output`;
the original-source oracle additionally requires the existing Python Params
and msgq extensions. `--case` selects one existing case without changing policy.

The new integration includes the unchanged planner state snapshot getter used
by the source comparator. Four existing trace examples now deserialize their
tagged operation first and then its typed payload, matching the already repaired
controller boundary for i128 identifiers. Production policy and strict source
comparison are unchanged; those four new wrappers await Actions execution.
Workspace formatting, metadata and CI policy checks (21 tests, 237 subtests)
pass. A local candidate rebuild is deferred under the recorded disk-space guard.
Its candidate build, host/ARM Actions and complete
normal startup/log-upload acceptance remain pending. No device timing, CPU
reduction or driving acceptance is claimed.

On 2026-10-08 this prepared candidate was composed with navigation integration
`eae4ac73` and its dev base `0e69a1c1`. Existing Navd, CarrotMan and RadarCAN
gates remain required. The independent 240-frame receipt above used the earlier
CXX-backed msgq boundary; the new host and native ARM jobs explicitly rerun the
actual IPC and lifecycle cases with current Rust msgq. Historical component
evidence is not relabeled as that new execution.

Local connection checks pass 16 workflow-isolation tests, 10 Card/RadarCAN
CI-policy tests, locked offline workspace metadata and diff whitespace checks.
The shared planner change only exposes a serialized state snapshot for the
source comparator. No radar detection policy or NAS replay source is changed.
This local integration preparation does not replace the pending exact-revision
host/ARM jobs or authorize device testing.

After Navd PR 227 passed every required gate, this candidate was merged with
dev `740ade11f2790efbb3c6aabdbb6e499acf3e8818`. The same 16 isolation tests,
10 CI-policy tests and locked offline metadata pass on the composed tree.
The included Card investigation record keeps issue 228's host stall unresolved;
its scoped unchanged retry passed, without changing the runtime or comparator.
Radard's new exact-head host/ARM jobs still must run before integration.

The first candidate's host Radard job passed, while native ARM run
`37722992726`, job `113134706879`, differed only at availability frame 16's
validity. The fixture scheduled this frame exactly 500 ms after the last
liveTracks input, at the original strict alive deadline. Three focused local
pairs using the retained host CI binary reproduced this difference; its direction
reversed in the third pair. Actual sender ages straddled 500 ms, and a source
SubMaster observation confirmed an actual receive age of 499.975436 ms with
valid output. Native receive age was not instrumented; sender and publication
times are retained as proxies rather than relabeled as receive timestamps.

The availability fixture now adds a 25 ms phase after input loss, keeping
subsequent 50 ms cadence and all packet data unchanged. Three complete 40-frame
comparisons pass with samples around 475 and 525 ms. A separate deterministic
state comparison checks receipt, deadline minus 1 microsecond, exactly the
deadline, deadline plus 1 microsecond and recovery; both original and native
alive values are `[true, true, false, false, true]`. Runtime policy, output
comparison and tolerances are unchanged. The existing state-probe binary was
reused locally after checking relevant source/schema/dependency identity;
both hosted architectures build it for their new required boundary comparison.
Receipts are in `.omo/evidence/229-radard-deadline/` in the integration checkout.
The corrected hosted jobs remain pending. A separate Card validity mismatch in
the common gate is under investigation and also prevents merging this candidate.
