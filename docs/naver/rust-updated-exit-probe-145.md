# Updater process observer exit race (#145)

[Issue #145](https://github.com/bin9208/openpilot-rust/issues/145) tracks the source/native process fixture failing when an owned worker disappeared during a `/proc/PID/stat` read. The [failed job](https://github.com/bin9208/openpilot-rust/actions/runs/36816686470/job/110223175555) raised `ProcessLookupError` after opening the stat file; this is distinct from a surviving process.

The observer now accepts `ProcessLookupError` alongside `FileNotFoundError`. It still rejects a live worker and propagates unrelated errors. A deterministic regression failed before the fix, then all three observer tests passed. The actual source/native updater command gate also passed merged output, nonzero status, preserved environment, and SIGINT cleanup with both live and exited leaders. No runtime updater code, wait limit or orphan assertion changes.

Local evidence is under `.analysis/scratch/2026-10-01-rust-gnss-integration/exit-probe-{red,green}.log` and `exit-probe-native/`. The regression runs through the existing Fast checks unittest discovery; required exact-SHA cloud validation remains the PR gate.

Docs-Not-Needed: test observer correctness only; no production or user-visible behavior changes.
