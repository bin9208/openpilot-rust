# Independent uploader and collector evidence (#511)

Issue: [#511](https://github.com/bin9208/openpilot-rust/issues/511).
Parent: [#497](https://github.com/bin9208/openpilot-rust/issues/497).
Related actual runtime correction: [#510](issue-510-uploader-pc-affinity.md).

The failed804 support artifact12470339923 exposed a separate capture defect.
`logmessaged_native.Peer.start` opens each collector's `stdout.log` and
`stderr.log` and keeps those file descriptors open. The Params/uploader checker
wrote the uploader subprocess bytes to those same paths before collector stop.
The original collector's later KeyboardInterrupt traceback could overwrite the
uploader prefix. The observed native scenario stderr contains that collector
traceback followed by a partial Rust backtrace; it is not a pristine child log.
The binary errorLogMessage independently preserves #510's actual affinity error.

The checker now writes `uploader-stdout.log` and `uploader-stderr.log`, preserving
the collector's existing files. It saves `uploader-exit.json` immediately after
the child returns and before return-code, IPC or normalized comparison assertions.
The receipt contains the actual argv/exit code, source/native role, child output
names, and the pre-launch Linux platform, /TICI marker and inherited CPU mask.
CPU affinity is queried, never changed by the checker. Collector `exit.json`
continues to record its own termination separately.

No return code, expected warning/error/log, HTTP/upload assertion, seven-case
matrix, collector combination, timeout, production runtime or input is changed.
Existing recursive support evidence upload includes the new child paths and
receipt even if a later assertion fails. Full Actions verification still needs
all28 original/native uploader/collector combinations and the remaining support
checks, required gates, normal integration and post-merge validation.

Local validation is Python AST syntax and diff review only. No local fixture,
project import, test/build or CPU/cgroup mutation was performed. Docs-Not-Needed:
this changes private QA evidence paths only, with no product option or user flow.
