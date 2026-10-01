# Rust workspace CI execution allowance

Issue: [#182](https://github.com/bin9208/openpilot-rust/issues/182).

The full Rust workspace job now needs to complete Clippy, workspace tests,
original-source comparisons, real IPC regressions and a release build. Its
45-minute maximum cancelled two runs without a compiler or assertion failure:

- dev `31d7306882218e9fecc44aba0d5c034f0d1ca188`:
  [36883744080](https://github.com/bin9208/openpilot-rust/actions/runs/36883744080),
  first attempt. Package setup took 14m36s; checks through msgq sanitizers passed,
  then the release build was interrupted by the 45-minute limit.
- Bluetooth PR head `df4bec339e04faaeeca222fd13e1556dfcb4c2c0`:
  [36884026408](https://github.com/bin9208/openpilot-rust/actions/runs/36884026408),
  first attempt. GitHub reports the same 45-minute limit. Attempt 2 passed.

The dev rerun spent 23m48s on package setup alone
(2026-10-01 16:11:46–16:35:34 UTC). Only this workspace job receives a 75-minute
maximum. Commands, checks, required gate aggregation, release build and all
other job timeouts are unchanged. This gives the existing complete validation
room to finish when dependency installation is slow; it does not guarantee
that package services always recover or replace evidence from successful runs.

Local validation checks the parsed workflow delta and the inherited workflow
policy. Exact-head PR and post-merge results are recorded in #182. No daemon
selection, runtime behavior, dependency version, vehicle or deployment changes.
