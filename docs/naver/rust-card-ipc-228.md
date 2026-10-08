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
