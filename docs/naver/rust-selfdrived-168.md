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

## Dynamic alert callbacks

All 24 callback kinds used by the source catalog now have native bodies. The
native code reuses the locale parser and original translation data, preserving
hardware-specific alert ordering, personality spelling, Params read order,
camera/process ordering, display rounding and ordered `max` behavior for NaN.
The source oracle executes the original callback and translation class bodies,
with constants and Hyundai flags read from their original definitions.

`callbacks-wire-final/manifest.json` records 15,852 cases across both hardware
layouts and 12 languages. The 15,648 alert-producing cases match all fields and
ordered parameter reads. Another 180 NaN/infinity cases reproduce the original
integer-rounding failure as an explicit native error. The original exceptions
are retained separately, alongside source/schema/translation and binary hashes.
Finite cases include adjacent binary64 values around rounding boundaries and
large magnitudes. The earlier `callbacks-boundaries` run completed comparisons
but failed writing provenance due to an incorrect schema path; it is retained.

The remaining 24 cases make `NNFFModelName` absent. The original callback
returns a null second text, and actual cereal wire assignment subsequently
raises a type mismatch. Native typed alerts reject that missing text during
callback resolution. Both reject the input, but the failure phase differs;
these cases are not claimed as exact callback-return or full-daemon equivalence.
Continuous-loop integration must retain the failure and its effect ordering.

All-target strict Clippy, formatting, Ruff and diff checks pass. The callback
comparison currently supplies the Params interface; the real Params adapter,
event generation, car-specific logic, calibrated pose and continuous daemon
are subsequent stages. This does not mark selfdrived ported.

## Physical alert Params adapter

The callback interface now has a native file-backed adapter. STRING reads reuse
the original-compatible UTF-8/logging boundary, integer reads reuse the verified
`std::stoi` semantics, and boolean reads match exact byte `1`. File read errors
produce the source getter's empty value; unknown keys and fatal integer casts
remain typed errors. No fatal cast is defaulted to zero.

`params-first/report.json` records 47 comparisons against the actual original
Cython Params binding, using synthetic files and private logging sockets.
Coverage includes the four callback keys, missing/empty/NUL/Unicode/malformed
UTF-8 values, permission/directory reads, signed limits and trailing integer
text, unknown keys, and seven invalid/out-of-range integer reads. Those seven
source child processes actually terminate with SIGABRT; the native adapter
returns its explicit fatal integer error for propagation by the daemon.
Warnings and their transported messages match the original. Strict all-target
Clippy, formatting, Ruff and diff checks pass. Continuous-loop effect ordering
and the remaining selfdrived policy are still in progress.
