`original.json` was captured by executing the unchanged standard-library-only
`check_managed_runtime.py`: its seven daemon/error-type definitions, nine actual
CLI outcomes, and four real cached-daemon observations (proclogd/uploader, each
with inherited context and omitted context). The cached binaries are explicitly
not claimed to match current source. A later fresh-source build of all seven
daemons and the native collector passed all 14 unchanged-original context cases.
The frozen native checker matched those exact result rows before and after
checker retirement; execve/execveat traces contained the same nine native ELFs
and no interpreter execution. No candidate policy provides the CLI or schedule
expectations.

`failures-original.json` preserves three actual unchanged-checker failure phases:
existing output, missing collector, and missing daemon after real collector
startup. It records artifact presence and observed IPC cleanup. Only controlled
temporary paths are replaced by labelled placeholders in exception messages.
The native tests preserve these expectations and require real ELF prerequisites;
missing artifacts are errors, never skips. Runtime receipts and raw logs stay in
private scratch.

`cli-edges-original.json` adds ten actual process controls for ambiguous prefixes,
help/error precedence, missing option values and invalid UTF-8 argv. Arguments
and process outputs are byte arrays, so no Unicode replacement can change the
original admission boundary. A separate private unchanged-original SIGINT
control verifies collector-startup cancellation, reaping and IPC cleanup; the
permanent real-process regression uses an immutable tracked shell launcher and a
read-only body, and exercises the native controller rather than a canned reply.

The permanent tests also cover Unicode decimal path admission, short-help
clusters, stale success invalidation, cancellation at the real final receipt
write, and rejection of zero-case selections. One ownership unit checks that
cleanup preserves a foreign replacement socket.

Independent review also reproduced FIFO executable prehash hangs and filesystem
inode reuse after collector closure. Permanent regressions cover both daemon and
collector FIFO paths, unchanged failure artifact phases, original listener
closure and foreign replacement preservation. The repaired checker passed all
14 original scenario rows again after deletion with zero interpreter executions;
its receipt is separate from the earlier before/after executable seal.
