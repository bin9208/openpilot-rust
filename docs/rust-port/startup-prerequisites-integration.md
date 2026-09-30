# Native manager startup prerequisites

Issue [#94](https://github.com/bin9208/openpilot-rust/issues/94) follows telemetry
integration [#83](telemetry-runtime-integration.md), under the approved
[full-runtime design](design.md). These libraries are intermediate components;
manager initialization/catalog, daemon adoption and complete normal startup
remain unported until separately implemented and observed.

| Input | Reviewed revision | Status |
| --- | --- | --- |
| Process supervision #84 | `097419b937c20ce216b190faf01c7f294a1ae4b7` | Parent production review, 3,555 hash checks and independent 43-scenario source/native replay pass |
| Registration #85/#88 | Pending | Source, local signed HTTP, charset and clock validation in progress |
| Managed child entry #86 | `592dd6839695c664dc8fae82aa2205189cb823f6` | Parent code review and independent44 source/native child comparisons pass through both original/native collectors |
| Checkout update status #92 | Pending | Source policy scoped; implementation not complete |

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
and checkout-status inputs will be added after their component review; the job
name does not claim that the normal manager or full startup has been ported.

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
