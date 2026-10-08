# Rust RadarCAN conversion (#193)

Issue: https://github.com/bin9208/openpilot-rust/issues/193

## Isolated dev integration, 2026-10-07

The integration candidate reuses the completed RadarCAN source and the #210
publisher-before-subscriber startup correction, after Card/CAN, planner and
native IPC. Existing full host/ARM source and memory evidence below is retained.
Only one real IPC composition was repeated locally: Volkswagen MEB, 65 input
steps and 61 exact full liveTracks publications, with both peers exiting -2.
The executed daemon SHA-256 is
`2893da6a78cfcf99880dcd0b9d748e981213807a5c849970f4b4ae0f338576da`.
Receipts and that executable are retained in
`.analysis/archive/2026-10-07-radarcan/`. Build, workspace formatting and CI
policy checks (31 tests, 215 subtests) pass. Existing host/ARM/memory jobs are
carried into the branch without removing earlier gates. Exact-head Actions
and dev integration are pending; production and NAS selection are unchanged.

This isolated candidate ports `radarcan`, its batch join, shared radar motion
filters, and the nine original brand radar interfaces. `radard` and `plannerd`
are separate components. The existing Python runtime, NAS replay service, and
production daemon selection remain unchanged.

The source is the original checkout's `openpilot/selfdrive/carrot/radar/` and
`opendbc_repo/opendbc/car/` implementation. Retain its MIT licensing and comma.ai
provenance. The checked NumPy/OpenBLAS artifact is an external native numerical
dependency; it does not supply project-owned radar policy or a Python runtime.

The first intermediate boundary implements the independent CAN/carState batch
join: 512 CAN packets, 32 states, strict 100 ms input age, original failure order,
and matching ego state for each packet batch. Four focused Rust scenarios and
15 actual-source/native traces match exactly, including retained queue contents.
The module-missing RED and passing results are retained privately under
`.omo/evidence/radarcan-193/` in the issue worktree. Executed batch probe SHA-256:
`6941f59a13031a21c49576e08505564be0fded0a5e1b489dae83ac6731187237`.

The common-filter boundary now has 40 exact source/native cases, including all
46 quadratic jerk matrices (7–52 samples), Python 3.12 sum behavior, ordered
NaN/signed-zero min/max, libm power rounding/overflow, and successful CAN address
arrival order plus CPython set merges. Fourteen Base cases preserve delayed ego
history, period estimation, partial failure state, copied publications, and
nonzero jerk. These results retain the original executed ELF hashes and source
capture identity; unchanged passing lanes are reused.

The first decoder boundary implements Chrysler, GM, Honda, Rivian, Tesla,
Toyota TSS1/TSS2, and Volkswagen MEB. Sixteen actual-source/native cases cover
616 full steps, including partial/reordered/duplicate/invalid CAN, lifecycle,
faults, and disabled radar. Source and native comparison includes the complete
common filter state, parser counters/timestamps/diagnostics, point order, and
backend state. The current retained decoder ELF SHA-256 is
`2c5425b03b70f91b88bb428ebaa50a25a7340c3fec372f2885995a0b73f26d57`.
Its 14 earlier decoder inputs/results were reused without changing their source
capture identity; MEB's two cases were newly executed against the original.

Ford MRR now matches 77 source steps. Its first native comparison exposed a
wrong translation of named DBC messages to numerical addresses; that failure is
retained, and the corrected complete scenario passes. Eleven NumPy cluster cases
match labels and every dot/distance float64 bit, including DDOT/GEMV/GEMM dispatch.
The new BLAS wrapper's 16 shape/stride/output-ownership fixtures pass native,
pinned Miri strict provenance, and Tree Borrows checks.

Hyundai's ten cases match 1,250 full steps across legacy SCC, optional 32/64-track
banks, camera SCC, Group4, CAN FD Groups1/2/3, corners235/180, corner-only input,
slot migration, and corner expiry. Comparison includes Params integer reads in
source order, all parser state, identity managers, filtering, and diagnostics.
Together the nine brand implementations have 28 cases and 1,943 full steps.
Evidence retains each original source capture and its executed binary identity;
the current daemon source is newer than these retained decoder snapshots.

Fourteen deterministic adapter cases execute the actual original `radarcan.main`
and match the native loop, including complete serialized liveTracks content,
interface recreation, error throttling, replay, copied flip, and fatal decode
phases. All CAN packets decode before joining; a malformed state preserves the
earlier decoded state. Error publications omit the points pointer, while an
explicit empty decoder selection retains it. Native package tests pass 17 cases,
and the retained loop probe SHA-256 is
`6c689497729fb38c7f93fd0b38d8e92512d3f6e50773893d19b1ad6886365e27`.
The adapter uses controlled clocks and IPC boundaries; it is not an actual
daemon, real IPC, scheduler, or performance validation.

The native daemon now builds and passes 18 package tests plus all-target Clippy.
Its retained host ELF SHA-256 is
`f3fe89e2d47613daa047a77787876981f987cbc7c7284b042a5a6a88409c8b0a`.
The corrected compiled source also matches all fourteen loop cases and the
affected Hyundai Group3/corner/SCC/fallback cases using unchanged source captures.
The scheduler's fully initialized FIFO51 argument fixture passes pinned Miri
strict provenance and Tree Borrows. These checks do not execute vehicle scheduling.

The current host daemon also passes 20 package tests and all-target Clippy, with
retained SHA-256 `b2624dc8d123ee1e06bd9df6ca4ef4370e92ebcf2ae58265efa743be4472daa0`.
Twenty-three actual source/native IPC profiles match 2,095 independently sent
input steps and 960 complete liveTracks publications per process. All nine
brands, Hyundai groups/corners/expiry, default fallback, MEB generations,
front flip and state-before-CAN delivery are represented. Actual clocks are
retained and bounded; publication content and validity compare exactly.
Eleven actual lifecycle cases cover both signals during CarParams waiting,
an explicit constructor setup barrier, and the ordinary unflagged input loop,
plus missing assets, negative history, unknown candidate, malformed CarParams,
and the inherited Params integer abort. The waiting-phase signal RED is retained;
the native correction matches Cython's explicit KeyboardInterrupt behavior.

The IPC fixture pauses only after actual construction and before initial timers,
then releases the two peers and independent producer. It requires an explicit
positive step bound. A prior ordinary cold-start run is retained separately:
the source produced 61 messages while this native debug artifact rejected early
queued states as stale and produced 60. Constructor timing differs; the setup
barrier does not establish normal cold-start equivalence or change the 100ms
bound. Final startup composition must still use continuously fresh inputs.

The reviewed native Rust IPC was merged at
`a885feba01db748650a4d05ba6f3edc68edeb45f`, preserving the Radar files and evidence.
The post-merge daemon SHA-256 is
`96a8d0ff06f744f0f8f4f063715bdb2f3fe9caedac94e7b0664c026efe5a02f6`.
Twenty-three Radar tests, seven CAN tests, native build and strict Clippy pass.
An explicit unavailable ESR metadata seed reproduced a native counter-creation
discrepancy: the source has no active counters, while the earlier native constructor
created 64. The source-derived RED and smallest guard correction are retained;
this seed does not add ESR assets or catalog support.

The new IPC daemon passes the fresh fixture recipe's 23 normal profiles (2,095
input steps and 960 publications per peer), five joined-input cases (164 steps,
32 separately prequeued states and 61 publications per peer), and eleven signal
and fatal-startup cases. Joined cases include partial CAN, missing metadata,
stale ego, state overflow, interface recreation and a latched front-only flip.
Complete publications and validity compare exactly; actual clocks and cleanup
remain separately observable. Prior C++ IPC artifacts retain their original
source/binary identity and are not presented as execution of the new backend.

The fresh CI recipe has required host, native ARM and four-level Miri jobs,
with explicit capacity guards, original source bindings, generated DBCs and
synthetic inputs, and always-retained failure evidence. The native ARM job uses
GitHub's documented `ubuntu-24.04-arm` runner so both original NumPy and Rust
use the same architecture and CPU dispatch. An actual original-source comparison
under cached ARM CPython 3.12.14 found 1,331 different jerk-weight bits across
all 46 matrices relative to x86 NumPy 2.5.3. This is numerical dependency context,
not a native error or an epsilon allowance. The existing cross/musl workflow
remains unchanged. Runner support is documented at
https://docs.github.com/en/actions/reference/runners/github-hosted-runners.

The new SVD buffer fixtures pass all four pinned Miri levels: default, strict
provenance/alignment, preemption and Tree Borrows. They cover all 46 runtime
sample counts and workspace-query/computation failures through injected ABI
callbacks. Foreign OpenBLAS code is not interpreted by these fixtures. A fresh
isolated Python environment installed all direct oracle pins from cached wheels;
the complete source-reference recipe passes with newly generated original DBCs
and the explicitly reused, unchanged source bindings.

The final read-error check found a Radar consumer mismatch tracked in
https://github.com/bin9208/openpilot-rust/issues/205. A CarParams directory made
the earlier native daemon exit with EISDIR while the original remained blocked.
The Radar-local getter now maps only filesystem read errors to the original empty
value behavior; integer conversion failures and other typed errors remain fatal.
Twenty-four Radar tests, seven CAN tests and strict Clippy pass, and fifteen real
source/native lifecycle scenarios pass, including both signals while waiting on
the directory, false flip and zero track enablement on directory reads.
The corrected host daemon SHA-256 is
`19aac96dd834c738b7acd0248667d11b536cb275fb8100ff8209fcf2080e28ad`.
The earlier complete host matrix is reused with its original executable identity;
the compiled source delta is limited to this IO-only adapter and its tests.

GNU ARM daemon, probe and test artifacts are retained. Thirty-one Radar/CAN
assertions pass across eleven QEMU test executables. Thirty-nine actual original
ARM CPython/NumPy versus ARM native cases match batch state, filters, track
lifecycle and every jerk-weight bit for all 46 matrices. This functional simulation
does not measure target performance. Full nine-brand source/reference and process
comparison on the native ARM hosted runner, independent review/integration and
exact-SHA hosted CI remain pending. This boundary does not establish
a complete Rust runtime, normal startup/upload readiness, AGNOS/device behavior,
CPU savings, or radar tuning.

Inherited source failures are preserved explicitly. In particular, the Ford ESR
source clears `updated_messages` before its ESR decoder iterates that set. The
current source Ford catalog uses MRR or no radar; any ESR metadata-seeded proof
must be labeled separately from catalog runtime support. Hyundai corner430 remains disabled by
the source's hard-coded policy. No missing assets or source algorithms are repaired
as part of this conversion.

## Integration resume, 2026-10-08

The preserved RadarCAN candidate is rebased by merge onto dev
`8482172a3b4e44d36de2bcdcb9508733d90be88e`, which contains native IPC and
CarrotMan. Both modules' workspace members and required CI dependencies remain
enabled. The connection-only checks pass: 16 isolation tests, 10 Card/RadarCAN
CI-policy tests, locked offline Cargo metadata and diff checks. Existing
independent process/numerical evidence above is reused; fresh hosted host/ARM
and required integration gates must pass before this candidate is merged.
