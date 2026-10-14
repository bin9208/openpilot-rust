# Card comparison input admission (#335)

Issue: https://github.com/bin9208/openpilot-rust/issues/335

Exact dev `a271bdf5312b7c2fd214e310394c6465ce68772d` failed the final Card
[pumped IPC comparison](https://github.com/bin9208/openpilot-rust/actions/runs/38624236116/job/116059219196).
Verified artifact11925807666 is197705239 bytes, SHA256
`2fa2811f757aea58507b7a38ab02248ab30ff42c8facd7ba499b5624dda9916d`.
This is distinct from #330: both Nissan X-Trail runs consumed80 CAN packets
one per iteration and produced80 outputs. After the existing normalization,
the only full-Params/publication difference was
`carOutput[47].actuatorsOutput.torque`: source−0.30000001192092896 versus
native−0.4000000059604645. Output validity indices were identical.

## Root cause and original-only proof

The pump publishes eight prebuilt topic events, then CAN, over separate sockets.
Original Card drains CAN and subsequently updates SubMaster. Its output reports
the previous apply's actuators. The Nissan controller copies input actuators
and changes steeringAngleDeg, so torque identifies the consumed control sample.

Source iteration46 drained CAN at2327663156138ns,34267ns before CAN47's
publish timestamp2327663190405ns. Its SubMaster update completed at
2327663650702ns; iteration47 reused that control receipt. Native iterations46
and47 admitted separate controls, with normal wall/CPU/stage timing. This is
consistent with source receiving control47 while processing CAN46, despite
retaining one CAN per iteration. The exact scheduling cause is not recorded.

An owned deterministic **original-only IPC** control established the mechanism:
with byte-identical retained inputs, one fixture fence admits control46 before
control47 is published; another holds SubMaster46 until control47 is published
while holding CAN47 until that update completes. Both execute unchanged original
Card/Nissan/SubMaster methods and pass the existing80-frame/one-packet/timeout/
validity checks. Admitted control IDs2321139279796 and2321139324703 map exactly
to−0.4 and−0.3 respectively; their prior-actuator output47 differs in exactly
the same single field. No numeric method or output field was changed.

The source control input SHA256 is
`5ba22ca0412b2c8766bb112443860466cbeb7058d807c64aec96508aef294201`.
Its first setup attempt stopped before the measured stream because a generated
DBC was absent locally; subsequent controls used the exact8812-byte CI DBC,
SHA256`5ad2711736bbc3a28da2c1db13a6fd0b3d0563ad60086f3b32dfda48ae1360a7`.
This control is deliberate boundary fault injection, not a proposal to slow or
gate the independent sender.

## Bounded comparison precondition

Each explicitly enabled frequency trace now records all nine subscribed topics'
seen/updated flags, receive frame and Event.logMonoTime. Eight topics are
published; customReservedRawData0 remains an explicit unseen sentinel. The
source wrapper records the same fields without replacing original methods.
The normal untraced native runtime gains no recording or policy changes.

Per case, all81 prepared input rows are encoded once. Each topic/positive event
ID binds uniquely to exact immutable wire bytes, SHA256 and the event's
exogenous valid bit. Every attempt receives the same frozen input bytes.
Duplicate/unknown identities, malformed/missing topics, invalid sentinel or
updated/retained transitions, non-contiguous/truncated traces, or changed input
files are hard failures. Updated=true with the same ID is allowed: warmup
deliberately republishes the same bytes.

The history covers frame zero through the measured stream's actual end,
including all post-constructor startup,320 warmup and any raw pre-window rows.
The end is derived from actual startup/warmup counts and measured-window
begin/end indices; there is no fixed two-startup-step requirement. Post-stream
shutdown rows are validated but excluded from comparison. Frame/receive-frame
indices share the post-constructor frame-zero origin. A fixed-clock original/
native state probe confirmed feasibility across402 frames and648 identities;
it does not claim the old unrecorded actual topic histories matched.

Every independent pair first completes all existing startup/readiness, warmup,
strict80 CAN timestamps/counts/timeouts, lifecycle/Params drain and exact CP
checks. Those failures never permit another attempt. Valid but different topic
histories are explicitly UNCOMPARABLE and permit at most three total independent
pairs. Exhaustion fails. The first matched history undergoes the unchanged full
output comparison; any difference fails immediately, without another attempt.
Derived frequency/alive/output validity differences are not discard criteria.
No best-output selection, field/frame removal, timing/validity tolerance, CAN
interval change, or receiver-driven measured sending was added.

All attempts and classification transitions are retained in separate directories,
with input identities, complete histories and mismatch receipts. Case paths do
not overwrite prior captures; a stale top-level PASS result is removed before
starting a new run. A PASS is written only after every selected case passes.

Existing startup CAN bootstrap batch counts legitimately differ (observed
Nissan[1,3]/[7,1], Mazda[1,3]/[82,1]); the observed startup step counts were two,
not a new contract. This change does not prove arbitrary bootstrap duplicates
inert across all platforms or identical complete initialization histories.
The existing fenced setup and exact warmup/stream CAN checks remain; a residual
bootstrap effect causing a matched-input output difference still fails.

## Validation

- 23 focused pure tests observed RED against fail-closed stubs before
  implementation. Expanded focused coverage includes admission validation,
  unchanged original SubMaster's46/47 boundary, validity lookup, variable
  startup length, all retry boundaries and matched-input torque tampering.
- Existing phase-fence regressions remain unchanged. Targeted Ruff passes.
  Final focused admission suite31 tests plus three existing phase-fence tests
  pass. This includes executing the real compare_case orchestration function
  with controlled80/CP/lifecycle/malformed-trace/output failures; each remains
  on attempt one and preserves its hard/output-failure classification.
- Native trace tests include a real nonzero control Event update, an empty
  update retaining its identity/frame, and a repeated update of the same ID.
  Four trace tests and strict Card native library/bin/tests Clippy pass.
- The first actual independent Nissan CLI comparison passed on attempt one,
  with402 complete matched history frames and all80 exact outputs. This was
  one fresh invocation, not repeated until pass.
- One full invocation using the exact CI18-candidate argument list passed all
  19 cases (both enabled/disabled COMMA_BODY),38 source/native captures and1520
  strictly compared measured frame pairs. Every case passed on attempt one;
  no input histories were discarded in this full run. This includes all
  existing readiness, warmup, packet/timeouts, complete Params, publication
  fields/validity and shutdown checks. The source startup observations remain
  evidence, not a new fixed initialization contract.
- An owned stale-result negative started with a PASS file and failed on a
  missing binding prerequisite: no PASS remained and prior capture evidence
  stayed unchanged. Final Ruff and `git diff --check` pass.

The validated host ELF SHA256 is
`637f66f81e83980f93d063cc139530d40efacdc27f21eed94a3c75939c596bd1`.
The full run used generated DBC/runtime-root and retained corpus from the
verified exact-dev artifact, with existing trusted host bindings/numerics;
there were no concurrent heavy builds. Native trace tests and source-only
orchestration controls cover diagnostic fields and eligibility independently
of the successful actual run. Exact final commit cloud CI remains required.

Ignored evidence: `.analysis/scratch/2026-10-15-card-admission-335/`, including
the deterministic original IPC proof, source/source differences, state probe,
TDD RED log, exact CI prerequisites and current comparison receipts. Builds used
the coordinated shared cache, two jobs and incremental disabled, with capacity
guards before builds/copies. No device access or publication; final exact-SHA
CI and complete candidate validation remain separate gates.
