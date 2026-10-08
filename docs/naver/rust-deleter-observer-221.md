# Deleter daemon checker stderr observation (#221)

Issue: https://github.com/bin9208/openpilot-rust/issues/221
Branch: `codex/fix-221-deleter-observer`. Integration: native IPC after Card PR #220.

The workspace low-space checker failed its unchanged 90–500 ms deletion cadence
gate after the source/snapshot reference lane had passed. Its stderr observer
combined OS-level `selectors` readiness with `Popen(text=True)` and buffered
`readline()`. A read of the ready marker could also pull the first deletion line
into Python's text buffer. The next selector event then released that buffered
line alongside a newer deletion, producing a near-zero observed interval even
though the child emitted deletions 100 ms apart.

`run()` now uses an unbuffered binary stderr pipe and explicitly decodes lines
with the original preferred locale encoding. Joined stderr retains universal
newline normalization. Readiness, signal timing, result fields, source policy,
snapshots and `.09 <= interval < .5` remain unchanged. Production daemon code
was not modified, and no tolerance or retry was added.

One focused regression calls the existing checker with a real Python child.
It emits ready and deletion 1 in one `os.write`, waits 100 ms between deletions
2 and 3, and records emitter times independently. It also checks complete
Unicode stderr, readiness and successful child exit.

Local invocation, with the already-installed environment:

```sh
PYTHONPATH=rust/tools:. /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-model-runtime/venv/bin/python -P -m pytest -c /dev/null --noconftest -p no:cacheprovider -q -s rust/tools/tests/test_deleter_observer.py
```

Before the fix, emitted intervals were 100.069/100.085 ms while observed
intervals were 0.025/100.052 ms; the existing 90 ms lower bound failed.
After the fix, emitted intervals were 100.075/100.075 ms and observed intervals
were 100.377/99.730 ms. The regression passed; focused Ruff and whitespace checks
also passed. Captured actual outputs are under
`.omo/evidence/deleter-observer-221/` and remain private working evidence.

The specified shared target's `debug/openpilot-deleter` executable was absent.
No Cargo build, install or real low-space daemon rerun was performed locally.
Actual low-space daemon/namespace validation and the exact-SHA hosted
workflow result remain parent-owned gates; this local result verifies the
observer defect, not a new production daemon result.
