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
## Alert storage and event dispatch

The native alert manager preserves insertion-order ties, priority/start-frame
selection, repeated-alert minimum duration, expiration and category clearing.
The original class bodies match all fields across 25,000 randomized steps.
Evidence is retained in `2026-10-01-rust-selfdrived/alert-manager-first`.

The complete 125-event catalog for both tici and mici is generated from the
unchanged source classes, static definitions and full cereal enums. Generated
data retains exact source/schema SHA-256 values. All 24 used callback kinds
are typed; callback bodies are a subsequent stage, not a runtime fallback.
The native event container matches 4,872 original-source steps covering sorted
duplicates, static persistence, counters, creation delays, category order,
callback dispatch, translation-copy behavior and hardware-specific definitions.
The callback boundary uses controlled alerts in this dispatch comparison;
it does not establish callback-body parity. See `events-final/manifest.json`.

Nine Rust behavior tests, strict all-target Clippy, formatting and Python Ruff
pass. Unknown schema enum values, missing catalog entries and callback failures
return explicit errors. No Python source executes in the native crate.
Continuous selfdrived, health gates and whole-runtime integration remain open.
