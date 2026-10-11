# Card host IPC investigation (#228)

Issue: https://github.com/bin9208/openpilot-rust/issues/228
Affected Navd PR: https://github.com/bin9208/openpilot-rust/pull/227
Failed head: `eae4ac73`.
Failed job: https://github.com/bin9208/openpilot-rust/actions/runs/37713528997/job/113104683263
Local source: `0e69a1c1f10bff3ddfb57f92d4cc9b1428710046`, branch
`codex/fix-228-card-ipc-pump`.

The hosted `CHEVROLET_VOLT-True-source` failure contains all 80 independent
CAN sends, at 10.121234..10.185117 ms intervals. Its 67 carState, carOutput and
sendcan publications account for all 80 packets: 66 states contain one packet,
and one state contains 14 packets with input ticks 29..42. The measured window
starts at event 0 and ends at 67; no leading/trailing publications are removed.
Native produced 80 states, each with one packet. This is a midstream source
receiver stall followed by the original `drain_sock_raw` backlog processing,
not missing input packets or the startup release ordering defect from #224.

The source carControl receive interval at frame 351 is 144.174042 ms. The
preceding sendcan publication is 2580023359279 ns; the next CAN receive is
2580165258307 ns. The archive does not contain syscall/scheduler captures that
distinguish notification waiting, scheduling delay, or work after publication.
Printed process maxima from earlier 100-frame windows do not cover that final
gap. Readiness and frequency checks remained true. Both peers completed the
expected SIGINT/empty-CAN timeout check and drained matching Params cache and
persistent writes. The origin of the hosted stall remains unresolved.

The bounded local investigation reused cached Python 3.12.14, NumPy 2.5.3,
the existing Cython Params/msgq bindings, and the exact archived input bytes
(SHA256 `47bd027204cb77050372e2f396395d03744e265b6dcc639fb64c2dae2d6270f9`).
No dependency was installed and no native package was rebuilt.

| Scenario | Observable | Local evidence |
| --- | --- | --- |
| Original affected source alone, unchanged pump/receiver | 80 states and 80 sendcan; strict timestamps, one packet per state, readiness and shutdown pass | `.omo/evidence/card-ipc-228/source-observed-invocation.json`, `source-observed/result.json`, original raw stream/frequency/lifecycle files |
| One explicitly injected owned source pause after tick 28 | 140.120095 ms SIGSTOP interval; 80 sends produce 68 states, one 13-packet batch spanning ticks 29..41; unchanged 80-state assertion fails at line 88 | `source-delayed-invocation.json`, `source-delayed/injected-delay.json`, `source-delayed.stderr`, raw stream and sends |
| Current source plus retained native affected pair | 80 states and 80 sendcan on each peer; complete CP/CS/prior actuator/CAN wire/validity comparison and original shutdown assertions pass | `affected-pair-invocation.json`, `affected-pair/result.json`, both raw captures/frequency/lifecycle files |

These artifacts are under `.omo/evidence/card-ipc-228/` in the assigned
`rust-carrot-man-219/openpilot-rust` checkout. `summary.json` records archive
and local packet ranges, actual pump/receiver intervals, lifecycle results,
and executable/binding/harness hashes. The two small reproduction drivers are
retained there as local evidence, with command/environment/stdout/stderr files.
The controlled pause proves detection of receiver backlog; it does not prove
what caused the hosted pause.

The retained native executable was read directly from
`rust-joystickd-208/openpilot-rust/.analysis/archive/2026-10-07-native-ipc/openpilot-card`;
its verified SHA256 is
`f47c17e80cb49eb0fefdf17a462c8a7adf2baaee4470d95be22e49cfead50baf`, matching
the retained receipt. The pair demonstrates compatibility for this Volt
fixture; it is not a fresh build of current dev. Native numerics reuse the
existing NumPy 2.5.3/OpenBLAS artifact at
`2026-09-30-rust-torqued/worktree/.omo/evidence/torqued-numpy253/numerics-host`.

No production Card/msgq or checked-in harness change is justified by the
available evidence. Independent 100 Hz sending, 80 timestamps, 80 states,
one packet per state, source/native equality, and shutdown assertions remain
unchanged. Inputs are not acknowledged per frame and batches are not hidden.
The issue remains unresolved. A single scoped hosted Card job rerun with these
strict checks was requested at the unchanged failed head. Attempt 2 passed the
Card job and aggregate at `eae4ac73fa60a733a131739dd328c63271861201`, allowing
Navd PR 227 to merge normally as `740ade11f2790efbb3c6aabdbb6e499acf3e8818`.
The passing retry does not resolve the origin of the hosted stall; issue 228
remains open. The local passing pair does not
replace the failed exact-head required Actions gate or establish device/CPU
behavior.

Docs-Not-Needed: investigation of an isolated host comparison failure; no user
setting or production behavior change.

## Chrysler validity observation on Radard PR 229

Run `37722992726`, Card job `113134706851`, retained a different failure in
artifact `11527642353`: `CHRYSLER_PACIFICA_2018-True` differs only at
`carOutput[33].valid` (source true, native false). The other eight captured
scenarios match. All nine have 80 sends/states/outputs and one CAN packet per
state, so this is distinct from the Volt backlog above.

Source CAN33 delivery takes 61.609078 ms, while the producer's next-send
interval also expands to 61.710723 ms. CAN34 follows source reception of CAN33
by 0.101645 ms, before the source's control read. Native delivery takes
0.078878 ms and its control read precedes CAN34 by 9.887712 ms. These clocks,
conflated carControl reception and the validity values support source consumption
of control34 versus native control33. That identity is inferred: the archived
trace does not record the consumed control stamp. The data establish neither a
receiver-only pause nor its scheduler/notification cause.

A bounded current Card build at `6bf088e588d831b03abc28d05a61b11150656b87`
produced SHA256
`8e418a68d4b33bb4fa7a39b7b05fc13777f937f80ba3e573be7421dcc370c4dd`.
One unchanged affected pair passes full output/Params comparison, 100 Hz input,
80 states/packets and shutdown assertions. One source-only observer repeat maps
all 80 consumed control stamps directly to their archived input indices;
frame33 consumes invalid control33 and publishes false. Neither local run
reproduces the hosted overlap. Local Python is 3.12.14 versus CI 3.12.15.
The current binary, exact inputs, DBC reuse and receipts are retained under
`.omo/evidence/229-card-validity/` in the integration checkout.

No Card runtime or checked-in comparator change is justified or made. The
passing local checks do not resolve the hosted pause or replace required CI.
Dev `740ade11` separately passes all post-merge checks, including its Card job
in run `37722579965`; the failed captures remain part of this open issue.

## Nissan native backlog on Carrot Navi PR 230

Head `9bd7c07f` passes the Carrot Navi, workspace and ARM jobs, but Card job
`113172735907` in run `37734997519` rejects `NISSAN_XTRAIL-True-native`:
80 independent sends produce 78 state/output/sendcan publications. Artifact
`11532004580` accounts for every input: 77 single-packet states and one
three-packet state spanning ticks 73..75. The measured window is exactly
events 0..77. Source has 80 single-packet states and a later empty-CAN state.

The native carControl receive interval is 39.782841 ms at frame 395, while
the pump's intervals remain 10.107603..10.247304 ms. Both peers pass readiness,
empty-CAN counter, expected signal exit and Params persistence checks. This
establishes a native receive backlog, not its scheduler/notification cause;
the archive has neither scheduler traces nor syscalls from that boundary.

One unchanged affected source/native pair passes locally in 15.24 s using the
retained `8e418a68` executable above and exact archived input SHA256
`4a3abdb22e67be371aedfc53e6e46a0d995e97740e4addfa3255c8eb013e99e8`.
All 80-send/state, one-packet, exact output/validity and shutdown assertions
remain enabled. The reused Nissan DBC is byte-identical to the hosted DBC.
Card, shared runtime and checked-in comparison sources are unchanged between
that executable's build and the failed head; it is not the hosted job's ELF.
No build, installation, production edit or comparator change was performed.

The failed archive, selected raw captures, journal, bounded replay invocation
and passing local pair are retained in the primary checkout at
`.analysis/scratch/2026-10-08-runtime-resume/230-card-failure/`.
The hosted pause remains unresolved. A single scoped retry of the failed Card
job at the unchanged head is the next gate; local success does not replace it.


## Nissan prior-output timing on WebRTC PR 262

Head `6ed320487962bf77788cb8486bb1b6e733cfb697`, Card job
[113900149326](https://github.com/bin9208/openpilot-rust/actions/runs/37953981375/job/113900149326),
retains eight pairs with 80 single-packet states each. Artifact `11630741423`
differs only at Nissan carOutput47 torque: source -0.30000001192092896 versus
native -0.4000000059604645. Source CAN46 arrives 44.252766 ms after sending;
CAN47 is sent 0.081836 ms after that reception and before the control read.
The source control receive interval is 54.458856 ms, followed by a step with
no new control. Native CAN46 arrives in 0.093172 ms with CAN47 another
10.072636 ms later. The prior-actuator publication and these clocks support
source consumption of control47 on CAN46 versus native control46; consumed
control identity remains inferred, and the hosted pause cause remains unknown.

One bounded local pair reuses the exact input and retained `8e418a68` ELF.
All 80-send/state/single-packet, Params, state, actuator, CAN and shutdown checks
pass. This is the earlier `6bf088e5` build, not the hosted ELF; the later Params
namespace-recreation patch is absent and unexercised here. Card/CAN/msgq/control
and checked-in comparator sources are unchanged. Removed old DBC and missing
torque-data fixture paths caused two setup failures retained separately; after
restoring owned fixture paths, the passing source capture was reused for the
native-only comparison. No build/install or runtime/comparator change occurred.

Evidence is in the RTC worktree at
`.omo/evidence/240-webrtc/exact-head-6ed320/card-summary.json` with the raw
archive, selected captures and invocation/exit receipts.
[Issue update](https://github.com/bin9208/openpilot-rust/issues/228#issuecomment-6085063169)
records the exact run and limitations. A scoped Card retry at the same head
remains a gate; this is not a fix for the hosted pause.

## ID.4 control overlap after webcam integration

Post-merge dev `35038bf600d3561e13038cc9b6dc1ab89a12904c` has the same
Git tree as the fully passing PR head `06937bf7193f7af3e8f1081d5dbfd55f210090b8`.
Its Card job [114499390259](https://github.com/bin9208/openpilot-rust/actions/runs/38149368079/job/114499390259)
passes all functional gates, then fails the actual host IPC comparison.
Artifact `11707181553` contains eighteen completed source/native pairs. Exact
normalized Params/state/output/CAN comparison agrees for seventeen; ID.4 MK1
differs at carOutput55 validity (source true/native false) and carOutput56 torque
(source 0.6000000238418579/native 0.5). All captured measured windows contain
80 states and the required single CAN packets; the failed pair's readiness,
empty-CAN counter, signal exit and Params drain pass on both sides.

Source CAN55 arrives 40.545876 ms after sending. The producer interval to CAN56
is 40.618851 ms, leaving 0.072975 ms between CAN55 reception and CAN56 sending.
CAN56 is sent before source carControl reception at 2456.684855775 s; that
receive interval is 50.976283 ms and the following step has no new control.
Native CAN55 arrives in 0.081157 ms; its control read precedes CAN56 by
9.687320 ms. These clocks, validity and prior-actuator publication support
source consumption of control56 during CAN55 while native consumes control55.
Consumed control identity is inferred, not directly recorded. The archive
does not establish the cause of the hosted pause.

No build, dependency install, runtime/comparator edit or repeated local matrix
was needed to classify this observation alongside the existing overlap cases.
The passing PR tree is unchanged. A single affected-job retry after the current
run finishes remains required; a passing retry will not close issue228.
The selected current captures, exact input and inspection notes are under
`.analysis/scratch/2026-10-12-card-228/`; the reproducible large archive may be
removed under the current user cleanup policy. No cadence, validity, frame-count,
packet-count, equality or shutdown requirement is relaxed.

The single retry completed successfully at the same merge SHA: Card job
[114509329939](https://github.com/bin9208/openpilot-rust/actions/runs/38149368079/job/114509329939)
passes functional, actual host IPC and shutdown steps, and workflow attempt2
finishes with all 46 jobs successful. Other passing jobs were reused. This
clears the exact-SHA post-merge gate without changing the source/runtime or
comparison requirements; the original failure and unresolved pause remain
recorded, and issue228 stays open.
