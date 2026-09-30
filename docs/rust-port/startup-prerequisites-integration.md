# Native manager startup prerequisites

Issue [#94](https://github.com/bin9208/openpilot-rust/issues/94) follows telemetry
integration [#83](telemetry-runtime-integration.md), under the approved
[full-runtime design](design.md). These libraries are intermediate components;
manager initialization/catalog, daemon adoption and complete normal startup
remain unported until separately implemented and observed.

| Input | Reviewed revision | Status |
| --- | --- | --- |
| Process supervision #84 | Pending | Source and real child/descriptor validation in progress |
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

Combined compilation, source/runtime CI, exact-head review and post-merge checks
remain pending. Native board/modem/spinner implementations and remaining runtime
components stay explicit. No production selection, vehicle connection, device
acceptance or CPU measurement is part of this increment. The user's first device
comparison follows the complete runtime, normal startup and existing upload path.

Docs-Not-Needed: internal runtime prerequisites and engineering evidence only.
