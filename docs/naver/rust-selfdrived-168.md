# Native selfdrived (#168)

The complete component is tracked in
[#168](https://github.com/bin9208/openpilot-rust/issues/168), under full-runtime
#1. Its preimplementation scope is in
[the runtime contract](../rust-port/selfdrived-contract.md).

The first verified stage implements the engagement state machine. All five
source states, ten event categories, disable/pre-enable/override precedence,
300-cycle soft-disable expiry and alert-category ordering are retained. Three
focused sequence regressions pass. An unchanged-source method comparison
passes 51,023 steps, including all 1,024 category combinations from each state
at six timer boundaries plus continuous/randomized sequences. The checker
reads DT_CTRL from the original realtime source. State, timer, enabled/active
flags and ordered alert categories compare exactly.

Local evidence is under the ignored `2026-10-01-rust-selfdrived` workspace:
`state-red.log` records the initial missing module, and `state-oracle-first`
retains inputs, independent source/native outputs and source/binary hashes.
Strict package Clippy, formatting, Python Ruff and whitespace checks pass.

Alert catalog/callbacks, event generation, car-specific policy, calibrated pose
checks, native continuous IPC/Params and full startup integration are still in
progress. This stage does not mark selfdrived ported, authorize production
selection or establish vehicle/performance acceptance. No device was accessed.
