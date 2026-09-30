# Continuous diagnostic log collector validation

Issue [#34](https://github.com/bin9208/openpilot-rust/issues/34), part of full
runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Implementation base: `277964e35f3a12037d143288d5641db3ed90e240` on `codex/feat-34-logmessaged`.

## Scope

`openpilot-logmessaged` continuously receives original multipart ZeroMQ PULL
records. Defaults preserve `Paths.swaglog_ipc()` and `Paths.swaglog_root()`:
`ipc:///tmp/logmessage${OPENPILOT_PREFIX}`, `/data/log/` when `/TICI` is a file,
and `${HOME}/.comma${OPENPILOT_PREFIX}/log` otherwise. `--log-root` and `--endpoint`
are explicit host QA overrides; optional `--frames N` counts all input records,
including records that exceed the publication limit. SIGINT/SIGTERM set a stop
flag; native receive waits at most 100 ms and also handles interruption.

The first joined byte is the level; all remaining bytes are decoded with UTF-8
replacement after joining multipart frames. Level >=20 invokes the disk handler
before the two-MiB **Unicode character** publication check. All records at or
below that limit publish `logMessage`; level >=40 also publishes
`errorLogMessage`. Both Events are valid and timestamped with the monotonic
clock. Rust `PubMaster` selects the original service catalog's 10,485,760-byte
queues. An empty joined record fails the process, as the original index access
does. No Params call is introduced on the infallible UTF-8 replacement path.

Swaglog uses the original defaults: 60-second rotation, a 262,144-byte size
threshold checked before the next write, ten-digit minimum numeric filenames,
and 2,500 retained files. Existing files are initially sorted ascending; new
files go at the front and retention pops the end. This intentionally preserves
the source's startup ordering quirk. Clock reads stay after file-position checks
and after closing the prior stream, preserving the timer origin across slow I/O. Prefix matches, Unicode numeric suffixes,
non-decimal-digit failures, and surrogateescape filename sorting are retained.

The formatter preserves Python JSON insertion order, last-value duplicate keys,
`msg` relocation, recursive dictionary type suffixes, untouched list contents,
null values, arbitrary-size integers, Python's default 4,300-digit integer input
limit, non-finite float tokens, lone surrogate escapes, ASCII escaping, spacing,
shortest decimal even-tie rounding, and a lowercase version-4 UUID hex ID.
Only the string-record path active in logmessaged is ported; producer-side
`SwagLogger` context and direct LogRecord formatting remain separate work.

## Error and resource behavior

Disk formatting, rollover and write errors are reported while the original raw
record still passes through the publication gates. Failed rollover opens leave
the old stream closed, matching the source. Buffered write/flush failure may
fail again during close and escape the daemon; it is not silently downgraded.
A previously closed rollover stream is ignored during close, matching the
original `finally` handler's ValueError case. ZeroMQ is closed before files.

Parsing, object-key conversion, dumping and destruction use flat nodes and heap
work lists. They do not use unbounded Rust call-stack recursion or impose a new
nesting policy limit. CPython recursion exhaustion is an implementation resource
boundary, not a logging policy: for a 1,500-level nested object the original
formatter raises RecursionError and skips its disk line, while Rust writes it.
Both processes still publish exactly the same original record on both services.
The native oracle checks this documented difference explicitly. A 5,000-level
object/array Rust regression exercises all four phases without stack overflow.
No exact arbitrary-depth or interpreter-resource-exhaustion parity is claimed.
Ordinary JSON syntax/type errors and tested I/O failure outcomes remain matched.

## Original-source and native evidence

The oracle imports the actual `SwagLogFileFormatter`/helpers and executes the
actual AST class for `SwaglogRotatingFileHandler`. Native QA also executes the
original `get_file_handler` and complete `logmessaged.main` bodies, adapting only
host paths. Reference policy/formatting/rotation logic is not copied into a
second expected implementation.

2026-09-30 host observations:

- **6,020 formatter records:** exact output bytes after replacing only the
  top-level generated UUID token; 6,000 seeded IEEE-754, big-integer and Unicode
  fixtures plus focused syntax, duplicate-key, surrogate and deep-array cases.
- **Nine rotating-handler scenarios:** exact filenames and normalized file
  bytes, emitted/error outcomes and close outcomes at size/time boundaries,
  retention startup, Unicode/invalid filename suffixes, failed opens and
  `/dev/full`. The UUID normalization leaves all other bytes unchanged.
- **Native original-vs-Rust daemons:** original Python ZMQ sender and original
  compiled msgq/cereal consumers verify level gates, split multipart frames,
  invalid UTF-8 and NUL, just-below/equal/above two-MiB records, Unicode records
  exceeding two MiB in bytes, disk-before-drop ordering, default 2,500-file
  retention, a real 61-second rotation, rollover and buffered-close failures,
  the documented deep-object difference, empty records and idle signal stops.
  Queue file capacities are inspected before the original subscriber can resize
  them. Full binary publications are retained; all decoded fields except the
  independently sampled monotonic timestamp match between separate processes.
  Each timestamp must lie in its real send/receive interval.
- **Ten Rust tests:** include failures observed before the suffix-collision,
  filename ordering, float tie and buffered-close fixes, plus separate rotation-check/open clock reads and deep lifecycle QA.
- Host fmt/clippy/test and Python lint pass. Existing native msgq C++ compiler
  warnings are preserved; no inherited native source was changed.
- Static aarch64-musl cross-build succeeds with the pinned Cargo toolchain and
  cargo-zigbuild. This is a build/ELF observation, not AGNOS or device execution.

Reproduce from the repository root with Rust 1.94.0, Python 3.12, capnproto,
numpy 2.4.6, pycapnp 2.1.0, pyzmq 27.2.0 and zstandard 0.25.0:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-logmessaged --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-logmessaged --examples --locked
PYTHONPATH=.:rust/tools python rust/tools/check_logmessaged_reference.py \
  --binary rust/target/debug/examples/file_probe --output /tmp/logmessaged-reference
python rust/tools/build_msgq_python.py --output /tmp/logmessaged-msgq
PYTHONPATH=/tmp/logmessaged-msgq:.:rust/tools python rust/tools/check_logmessaged_native.py \
  --binary rust/target/debug/openpilot-logmessaged --output /tmp/logmessaged-native
cargo zigbuild --manifest-path rust/Cargo.toml -p openpilot-logmessaged --release --locked \
  --target aarch64-unknown-linux-musl
```

`rust checks` repeats both original-source and native checks. The existing
workspace aarch64 build includes the daemon and its vendored native dependency.
The [third-party record](../../rust/crates/logmessaged/THIRD_PARTY.md) retains
libzmq's LGPL/linking-exception notice and binding licenses. Native libzmq 4.3.4
is built through locked `zmq` 0.10.0 / `zmq-sys` 0.12.0 / `zeromq-src`
0.2.6+4.3.4; original msgq remains an external C++ dependency. There is no Python
call in the Rust runtime.

Local evidence lives in the issue worktree's `.omo/evidence/logmessaged/` and
parent archive `.analysis/archive/2026-09-30-rust-logmessaged/issue34/`.
`index.json` records the implementation SHA, commands, source/binary/artifact
hashes, binary observables and limitations. Synthetic payloads and raw captures
remain outside Git.

## Remaining delivery gates

Parent-owned exact-head Actions and independent review remain required before
integration. Production manager selection is unchanged. Route loggerd, upload
services and complete startup integration remain separate full-runtime work.
No vehicle was accessed; no device behavior or CPU saving is claimed. The full
runtime candidate must be completed before the user's first device comparison.

Docs-Not-Needed: isolated Rust diagnostic daemon and parity tooling; no existing
production selector, user setting or user-visible setting behavior changes.
