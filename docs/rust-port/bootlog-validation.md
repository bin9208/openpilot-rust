# Native boot log and Params snapshot

[Issue #97](https://github.com/bin9208/openpilot-rust/issues/97) ports
`openpilot/system/loggerd/bootlog.cc` and only `save_bootlog` from
`openpilot/system/manager/helpers.py`. Original MIT licensing and source files
remain unchanged. This increment reuses native loggerd `Environment::init_data`,
Params, the shared identifier parser and the libc buffered file owner.

The boot executable increments BootCount, opens `boot/<identifier>.zst`, writes
initData followed by Boot, and records CurrentBootlog before final compression
and close. Boot contains pstore entries in byte-key order, the original journal
shell command and launch log bytes. Missing/unreadable optional files become
empty bytes. Journal exit status does not discard stdout. The original
`fgets(128)` and NUL-termination behavior is retained. initData reads the copied
Params namespace while BootCount and CurrentBootlog update the live namespace.
The existing native initData provenance entries identify the Rust implementation.

[Issue #99](https://github.com/bin9208/openpilot-rust/issues/99) records a shared
identifier discrepancy discovered by the boot oracle: original identifier
creation treats a failed counter read as empty and ignores a failed counter
write. The narrow correction applies to both RouteCount and BootCount, retaining
parsing, unsigned wrapping, write-before-random order and all other Params error
behavior. The route wrapper continues to select RouteCount.

The source zstd writer buffers input until the recommended stream input size.
Its final `fflush` return is ignored before checking `fclose`. The boot writer
reuses the existing unique-owner libc FILE adapter with a checked-close method;
the raw VideoWriter's existing finish policy is unchanged. A host `/dev/full`
fault fixture distinguishes small buffered final output, which can report
success after setting CurrentBootlog, from a large failed write before that
update. This preserves an inherited failure boundary, not a reliability claim
about storage. The existing route compressed writer is outside this boot-specific
buffering change; its independent behavior is not certified by the boot oracle.

`Snapshot::capture` copies Params synchronously into a private temporary root.
File/directory copies follow symlinks and preserve permissions, timestamps and
extended attributes. Copy errors retain the temporary tree. `launch` runs the
worker on a detached thread; `save_bootlog` exposes the combined operation and
returns a handle for an optional caller join. The worker inherits the environment,
overrides PARAMS_COPY_PATH, uses the loggerd directory as cwd, and removes the
snapshot after normal child return, including a nonzero child status. A missing
binary still triggers cleanup. Spawn failures leave the snapshot, matching the
source helper's absence of a finally block. Dropping the handle does not hold
process exit open. Cleanup failures remain observable. Detached worker errors are written to stderr,
while an explicitly joined handle also returns the typed error.

Parent integration additionally covers readable character-device links, inherited
descriptor closure and long temporary paths. The snapshot root is created with
explicit `0700` permissions, matching Python `mkdtemp` (#112). Child execution uses
the native process helper with a child-only PARAMS_COPY_PATH override. Ten actual
source/native snapshot scenarios and four focused Rust tests pass after these
corrections; the earlier failing comparisons remain in local evidence.

## Reproduction and evidence

Private evidence is indexed in `.omo/evidence/bootlog/evidence.json`, with exact
commands, source and binary hashes, failed comparisons, decoded records and
source/native results. `check_bootlog.py` executes the unchanged C++ implementation
and native executable. C++ wrappers redirect only the external pstore and launch
file paths; actual source readers, journal shell command, Params and compression
execute. Controlled `df`/journal programs and synthetic Params avoid recording
host journal or device data. Full cereal decoding verifies both events, and the
unchanged uploader selection chooses the resulting compressed boot artifact.

`check_bootlog_snapshot.py` executes the unchanged Python function with the
actual Params extension, real daemon threads and real children. Pipe handshakes
prove return-before-completion, snapshot isolation, child environment/cwd and
cleanup order without sleeps. `check_logger_identifier.py` compares the actual
C++ shared function for RouteCount and BootCount, including directory, unreadable
and unwritable counter cases. Existing loggerd tests and route scenarios are
separate regressions for shared changes.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-bootlog -p openpilot-loggerd --all-targets --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-bootlog -p openpilot-loggerd --all-targets --locked -- -D warnings
python rust/tools/check_bootlog.py BOOTLOG ORIGINAL_BOOTLOG OUTPUT --fault-library OUTPUT_FAULT_SO
python rust/tools/check_bootlog_snapshot.py SNAPSHOT_PROBE ORIGINAL_PARAMS_BINDING OUTPUT --launcher PROCESS_CHILD
python rust/tools/check_logger_identifier.py IDENTIFIER_PROBE ORIGINAL_IDENTIFIER OUTPUT
```

The native executable defaults to `/sys/fs/pstore` and `/tmp/launch_log`; explicit
`--pstore PATH` and `--launch-log PATH` arguments permit controlled fixture inputs.
They do not change manager selection. The snapshot library accepts the existing
Params namespace, loggerd directory and native process-helper path; manager
integration supplies those dependencies explicitly.

## Remaining gates

No production daemon selector or original runtime source is changed. No vehicle,
real journal capture, real persist key or host system path mutation is performed.
Native dependencies include libc buffered I/O, zstd, clocks, filesystem/xattrs,
`/bin/sh` and journalctl, plus loggerd's existing FFmpeg/msgq/libzmq dependency
closure. These are not represented as Rust rewrites. Host and generic ARM checks
do not establish AGNOS startup, full runtime/log-upload comparison, device
acceptance or CPU savings. Parent exact-SHA integration and cloud gates remain
separate. Issues are not closed by this local increment.

Docs-Not-Needed: internal runtime component and host verification, with no user
setting or production-selection change.
