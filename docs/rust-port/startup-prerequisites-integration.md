# Native manager startup prerequisites

Issue [#94](https://github.com/bin9208/openpilot-rust/issues/94) follows telemetry
integration [#83](telemetry-runtime-integration.md), under the approved
[full-runtime design](design.md). These libraries are intermediate components;
manager initialization/catalog, daemon adoption and complete normal startup
remain unported until separately implemented and observed.

| Input | Reviewed revision | Status |
| --- | --- | --- |
| Process supervision #84 | `097419b937c20ce216b190faf01c7f294a1ae4b7` | Parent production review, 3,555 hash checks and independent 43-scenario source/native replay pass |
| Registration #85/#88 | `bfa086fe1e830649c377483dd0009cdfe8e96a31` | Parent review,3,705 hash checks and independent119-case source/native replay pass |
| Managed child entry #86 | `592dd6839695c664dc8fae82aa2205189cb823f6` | Parent code review and independent44 source/native child comparisons pass through both original/native collectors |
| Checkout update status #92 | Pending | Source policy scoped; implementation not complete |
| Manager catalog #96 | `b283dbeca022a5c85a9e3554201743a0f7c98fa3` | Parent review and full independent63-entry/10,573-predicate source matrix pass |

The managed-entry input preserves stage order, concrete Rust error information,
interrupt warning and crash/Params/SDK failure ordering. Parent independently
checks21 source/executable hashes and957 artifact references, then uses the
preserved executables with newly built original IPC/Params bindings and pycapnp
2.1.0. Its normal manager adoption, native argv identity, blocking cancellation
and panic/thread limitations remain in [the component record](managed-entry-validation.md).

Inherited source defect [#90](https://github.com/bin9208/openpilot-rust/issues/90)
can launch duplicate persistent children after PID storage failure. Its source
parity observation is not a repair. Registration's external cookie mismatch
[#88](https://github.com/bin9208/openpilot-rust/issues/88) is tracked separately.

The supervision input preserves actual child execution, stdio, descriptor and
process-group differences, stop/restart order, persistent PID reuse and errors.
Its component host and generic ARM oracles each pass 43 scenarios. Parent repeats
the host oracle with independently built source bindings, using frozen component
binaries, and verifies all source/artifact/binary ledger hashes. The merge adds
only nix 0.31.3 and the workspace package to the lockfile; all 371 existing
package records remain identical. Full catalog and manager adoption stay open.

Combined compilation, source/runtime CI, exact-head review and post-merge checks
remain pending. Native board/modem/spinner implementations and remaining runtime
components stay explicit. No production selection, vehicle connection, device
acceptance or CPU measurement is part of this increment. The user's first device
comparison follows the complete runtime, normal startup and existing upload path.

The required `rust startup prerequisites` CI job now builds real child executables
and original message/Params bindings, then runs the supervision and managed-entry
comparisons against those artifacts. The aggregate `rust checks` gate requires
that job to succeed; failed, cancelled, skipped and missing results are rejected.
Raw comparison and binding evidence is retained even after failure. Registration
and catalog comparisons are now included; checkout-status is still pending.
The job name does not claim that the normal manager or full startup has been ported.

The registration input passes119 scenarios each on host and generic ARM, three
real fifteen-second HTTP/progress scenarios,13 clock and107 UTF-7 cases on each
architecture. Parent repeats the119-case matrix with a fresh pinned environment
and independent original bindings. The detector's28-file published archive is
verified in CI, allowing only its documented manifest compiler-version change.
The shared uploader signing regression remains required. Merge preserves every
existing dependency version; metadata and tombstoned dependency references are qualified as
`unicode-general-category 0.6.0` because the detector introduces version1.1.0.
The first locked combined build caught the remaining unqualified tombstoned
reference. Cargo's host-target resolution corrects only that reference, with
no package/version changes; the failed build output is retained.

The catalog input compares all63 registered entries,64 import configurations,
25 actual environment snapshots,10,573 predicate/access/Params-mutation cases
and24 fatal integer cases on each architecture. Parent independently repeats
the full host matrix with freshly built original bindings and verifies22 source
and executable hashes plus709 artifact references. Candidate metadata lists15
isolated Rust implementations and48 unported entries at this revision. Typed
fatal errors must reach the later manager entry boundary; the library API is
not evidence of identical process abort behavior.

The first combined supervision run exposed a test synchronization defect: the
child's fixed 200 ms delay could expire between the two stop requests, making a
correct immediate second return fail the test's minimum-duration assertion.
The preserved failure shows exit code 0 and the expected cleared process state.
The comparison now holds the real child until an explicit atomic release,
acknowledges receipt of the second request and verifies it cannot return while
the child remains held. All 43 source/native scenarios pass with that barrier.
Only the test RPC/example changed; production stop behavior is unchanged.

## Local build capacity

The user's low-space warning is tracked in
[#95](https://github.com/bin9208/openpilot-rust/issues/95). With all active workers
held, two verified cleanup passes reclaimed 24.57 GiB from 33 already merged
worktrees and restored 35.53 GiB of available storage. Sources and Git status,
active build caches, daemon/example executables, dynamic libraries, bindings,
logs and reproduction evidence were preserved. Private local inventories retain
the removed paths and verification results; raw local evidence stays outside Git.

Before local build, install or large-copy work, reserve 25 GiB plus expected
growth. If below that floor, recover at least 35 GiB before resuming. Use bounded
package builds and coordinate reuse of inactive caches after freezing required
evidence. This engineering resource rule changes no runtime behavior or CI gate.

Docs-Not-Needed: internal runtime prerequisites and engineering evidence only.
