# Continuous runtime integration increment

Issue [#39](https://github.com/bin9208/openpilot-rust/issues/39) integrates the
verified monitoring policy and five continuous daemons while full-runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1) remains open.

| Component | Issue | Implementation evidence |
| --- | --- | --- |
| Driver monitoring policy | [#29](https://github.com/bin9208/openpilot-rust/issues/29) | [209144 source steps and complete packets](monitoring-validation.md) |
| Internal driving daemon | [#31](https://github.com/bin9208/openpilot-rust/issues/31) | [90 native publications and original model outputs](driving-daemon-validation.md) |
| Diagnostic log collector | [#34](https://github.com/bin9208/openpilot-rust/issues/34) | [Formatting, native input and real file rotation](logmessaged-validation.md) |
| Calibration daemon | [#35](https://github.com/bin9208/openpilot-rust/issues/35) | [Estimator, native loop and blocked persistence](calibration-validation.md) |
| Monitoring daemon | [#38](https://github.com/bin9208/openpilot-rust/issues/38) | [437 native packets and post-publication Params](dmonitoring-daemon-validation.md) |
| Log-space deleter | [#42](https://github.com/bin9208/openpilot-rust/issues/42) | [Original retention and native low-space cleanup](deleter-validation.md) |

The integration branch preserves each reviewed feature commit. Shared Cargo,
workflow and inventory conflicts retain both sides' components and checks.
One combined PR validates the interaction on a single revision, followed by
separate dev push checks. Earlier component checks do not substitute for those
combined checks. PR and Actions links are recorded on #39 after publication.

The first combined push exposed an equal-deadline assumption in an existing
native IPC test. [Issue #47](msgq-handshake-validation.md) records its controlled
reproduction and send-acknowledgement correction. Production transport is
unchanged; the final revision repeats all gates.

On `afe30750`, push Rust run 36670554318 passed. PR run 36670557576 passed
ARM, memory and model pipelines, plus every fast-job test and the release
build, but hit the 15-minute job limit during its final CPU-sample command.
The fast job now has a 20-minute orchestration budget. Test assertions,
source comparison tolerances and runtime deadlines are unchanged; the new
head must pass all required checks before merge.

The common runtime Params constructor also replaces duplicated driving and
calibration constructors so an explicitly empty `OPENPILOT_PREFIX` follows the
original root namespace. Unset and named prefixes retain their source paths.
Subprocess regressions exercise each case without changing concurrent test
process environments. Parser and asynchronous persistence semantics stay intact.

Parent native-surface reviews reran each daemon's real IPC scenarios. Evidence
and captured packets are indexed in the corresponding private analysis archives;
they contain synthetic host inputs. The first user device comparison still waits
for complete startup, every project-owned runtime service, ordinary route logging
and existing upload integration. Production process selection is unchanged here.
Host or cross-build success establishes no vehicle behavior or CPU improvement.

Docs-Not-Needed: internal runtime integration; no production selector or user
setting behavior changes.
