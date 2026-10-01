# Uploader logging validation readiness (#172)

[Issue #172](https://github.com/bin9208/openpilot-rust/issues/172) tracks
[PR #167's failed support job](https://github.com/bin9208/openpilot-rust/actions/runs/36863031862/job/110372448408)
at `bc7623f7fa89e0611ca8e5efab0a02bcd0e55ded`. The first getsize-error
record appeared on disk and in `logMessage`, but not `errorLogMessage`.

The validation peer previously considered shared-memory file creation sufficient
for readiness. Original msgq creates and sizes the file before initializing
its publisher, which resets subscriber registration. A reader registered in
that interval can lose the first publication if it waits until after the
upload operation to receive. The original collector writes the disk record
and both publications sequentially; the uploaded artifact alone did not prove
which scheduling interval occurred.

`check_logmessaged_startup.py` builds a small owned LD_PRELOAD fixture that
pauses the original collector after sizing `errorLogMessage`, before publisher
initialization. The legacy readiness control deterministically records one
normal publication and zero error publications. The repaired source and native
cases each record one of both. This reproduces the CI failure shape, but does
not establish the precise scheduling of the earlier run.

Uploader logging validation now requests an explicit DEBUG message round trip.
Acknowledging the newest marker proves earlier markers were consumed and both
publisher constructors finished. A nonblocking error read then refreshes any
registration invalidated during startup. Readiness packets remain separate from
test publications, and DEBUG records do not enter the disk log. Bounded-frame
collectors cannot use this optional handshake. Production daemons, expected
record counts, and existing test drain deadlines are unchanged.

Local evidence under the root ignored `2026-10-01-bluetooth-ci` directory:

- `publisher-red-late/observed.json`: controlled legacy failure.
- `publisher-regression-fixed/report.json`: legacy failure control and repaired
  source/native scenarios pass; disk contains exactly the intended error record.
- `logging-current-green/report.json`: all 36 uploader/source collector
  comparisons pass, including original message fields, severity, disk formatting,
  source callsites and error meaning.
- `logging-green/report.json`: earlier run with preserved older binaries failed
  the source line-number assertions; those binaries were not from the current
  uploader source. The failure was retained and rerun using the current cache.

The regression is included in the support runtime CI job. Ruff and diff checks
pass locally. Exact-head and post-merge CI are required before closing #172.
This is test infrastructure evidence; no vehicle or external upload endpoint
was accessed.

Docs-Not-Needed: validation lifecycle repair only; no user setting changes.
