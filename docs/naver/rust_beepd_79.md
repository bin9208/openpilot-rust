# Rust beep support runtime increment

Tracking: [#79](https://github.com/bin9208/openpilot-rust/issues/79), within
[full-runtime #1](https://github.com/bin9208/openpilot-rust/issues/1).
Branch: `codex/feat-79-beepd`; base: `f2779c72`.

The optional native GPIO alert daemon preserves the original startup, alert
mapping, per-pulse volume reads, overlapping daemon threads and 20 Hz loop.
See [the validation record](../rust-port/beepd-validation.md) for the real source,
Params, IPC, command and lifecycle surfaces and their limits. Exact tested commit,
source/binary hashes and captured artifacts live in `.omo/evidence/beepd/evidence.json`.

Actual Cython `get_int` aborts on malformed or overflowing values; this inherited
source defect is tracked independently in [#82](https://github.com/bin9208/openpilot-rust/issues/82).
The native port reports a typed fatal error and exits nonzero, rather than hiding
the failure or fabricating a conversion warning. Python/Cython source is unchanged.

GPIO42 sysfs, sudo/tee, the shell and original msgq remain external dependencies.
No production manager switch, real sound/GPIO contact, device installation or
whole-runtime completion is included. Cloud validation and dev integration belong
to the parent task; no remote CI success is claimed by the local evidence.
