# Independent QCOM assistance checker observations

Captured the unchanged `rust/tools/check_qcomgps_assistance.py` before any native
checker implementation. Checker SHA256:
`6fb7f5f62070d99d2c7443ec97467619b96d46e2031cba8da750d123caf53805`.
All four original scenarios passed with the existing host daemon: alternate
replacement without HTTP, accepted 404 payload, rejected oversized download and
one retry after the loopback peer closes its first connection. Four actual early
failure controls and five CLI controls retain admission and output-file phases.

The preexisting daemon's hash and original runtime-source hashes are retained in
ignored `.analysis/scratch/2026-10-18-qcom-assistance-tools/original-inputs.sha256`.
That baseline does not claim a fresh build of the present source. Final native
comparison requires a newly built source-matched daemon and owned loopback-only
execution. No GPS/modem/vehicle device is used. All fixtures are synthetic.

The final native checker retains all four actual scenarios, exact artifact bytes,
retry/count/status behavior,15-second observation and3-second shutdown deadlines.
Its seven focused server regressions preserve GET-only admission, original65,536
byte request/header line limits and100-header-list bound (including the final
blank line), plus an original-valid header pause beyond one second. Active
socket cancellation bounds owner shutdown without adding a per-read/write
timeout. Accepted-handler trickle controls exercise both finish and Drop with
owned client/thread cleanup before assertions. Production never reads these
expected fixture objects.

Parent's final source-matched gate passed80 library tests plus all seven actual
QCOM integration tests, strict Clippy and the checker build. The frozen empty-PATH proof exactly matches all four freshly captured original
result objects and passes five CLI, four failure and five source-guard controls.
Its five execs use only the checker and sealed daemon ELF, with no Python.
After deleting the original checker, the same proof passes again unchanged
(actual-native-4gemcmt4, following actual-native-1uyubxym before deletion).
The seven actual integration tests also pass with the original file absent. No GPS/modem/device execution
or CPU saving is established by these loopback/file checks.
