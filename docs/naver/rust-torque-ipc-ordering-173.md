# Torque IPC fixture batch boundary (#173)

Controls PR #164 at `4d10a67b7274e568b99be2444216bdfcef23d2e9` failed
[run 36866943989](https://github.com/bin9208/openpilot-rust/actions/runs/36866943989),
job `110384977665`, during persistence comparison: native bucket count 4080,
source 4081. The original CI job did not preserve partial torque artifacts.

The fixture publishes six topic messages and applies the same list atomically to
the original-source oracle. It previously waited only for the livePose reader.
Both source and native transport consume the polled queue before unpolled queues.
Acknowledging livePose therefore permits a following synthetic batch to replace
an unread conflated carOutput, leaving different input histories in the two sides.

Temporary native instrumentation paused after the 99th poll consumption, before
reading other topics. The unchanged fixture reproduced exactly 4080 versus 4081
at frame 100. Native frame 99 reported `updated=false` for every non-poll topic.
The fix waits for all six input readers before sending the next batch. The same
instrumented binary then passed the full cadence/persistence/lifecycle checker.
Instrumentation was removed, the native daemon rebuilt, and the full checker
passed again. This establishes a concrete fixture race; it does not establish the
unrecorded scheduler interleaving of the historical CI failure.

`check_torque_ipc_ordering.py` retains a deterministic regression using original
native msgq sockets and an explicit process barrier after polled consumption.
The legacy wait produces frame identities `{livePose:1, others:2}` then
`{livePose:2}`; the fixed wait preserves all six topics at frame 1 and frame 2.
No arbitrary delay, retry, numerical tolerance or production threshold is added.
On comparison failure the fixture now records the source frame and full expected
and actual messages. CI uploads torque evidence even when the check fails.

Local evidence is retained under
`.analysis/scratch/2026-10-01-controls-athena-forwarding-ci/`:

- `torque-controlled-red/`, its log and `proof/instrumentation.patch` reproduce
  the pre-fix failure. Preserved instrumented binary SHA256:
  `c3bc2c48eb9d82c4145e118648c44dc8098b46c1a223533b84cd9a15e45fb650`.
- `torque-controlled-green/report.json` and `torque-restored-green/report.json`
  record complete native IPC, cache persistence, cadence and lifecycle passes.
- `torque-ordering-first/report.json` records both deterministic queue sequences.
- Strict Ruff, workflow-policy unit checks and `git diff --check` passed.

Reproduce the regression with the existing original msgq Python extension on
PYTHONPATH: `python rust/tools/check_torque_ipc_ordering.py --output FRESH_DIR`.
Run `check_torque_daemon.py` with the native binary and pinned torque numerics as
in `.github/workflows/rust.yml`. No production Rust/C++ changes remain in this fix.
Exact-SHA PR/push/post-merge checks remain required before closing #173. Full
runtime startup/upload and device comparison remain #1 acceptance gates.

Docs-Not-Needed: host validation and failure evidence only; no user setting,
production process selection or runtime policy changes.
