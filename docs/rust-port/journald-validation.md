# Journald native runtime

Issue: [#65](https://github.com/bin9208/openpilot-rust/issues/65), under full-runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1) /
[#6](https://github.com/bin9208/openpilot-rust/issues/6).
Source baseline: `7bf702067bb7b76ee81818ba3f1a899a9aad3d6f`,
`openpilot/system/journald.py`. Original licensing and source remain unchanged.

## Owned behavior and dependencies

`openpilot-journald` / `journald-rs` owns the existing journal bridge: the exact
`journalctl -f -o json` child invocation, UTF-8 pipe reads and universal newlines,
ordered JSON conversion, `androidLog` publication, malformed-JSON diagnostics,
and child terminate/wait cleanup. OS `journalctl` and the journal service remain
external dependencies. Original C++ msgq and libzmq remain the existing narrow
transport dependencies. Python is used only by build/validation oracles.

The bridge preserves `valid=false`, monotonic event timestamps, timestamp units,
Int32 PID / UInt8 priority / UInt64 realtime ranges, absent versus explicit empty
tag pointers, record order and source JSON formatting. Typed values retain
insertion order, duplicate-key replacement position, arbitrary integer text,
Python float rendering, ASCII escaping, surrogate code units and the source
NaN/Infinity JSON extensions. The decoder uses serde_json's number grammar and
string decoding; a separate raw-string validation step rejects unescaped
controls that serde_json's byte-string interface otherwise permits.

Integer conversion preserves truncation, booleans, optional signs, underscores
and Unicode 15.0 decimal digits from the source CPython 3.12 runtime. Integer
string whitespace differs from outer `line.strip()` whitespace; the regression
cases preserve that distinction. Default Python's 4300-digit JSON integer limit
is a fatal conversion error. Parsing, serialization and value destruction use explicit heap worklists; no
arbitrary nesting cutoff or recursive Rust walk rejects source-valid deep input.
A 5000-level record is verified through the actual original and native processes.

Only JSON syntax failures emit ERROR `failed to parse journalctl output` and
continue. Root-type, conversion/range, UTF-8 and publication failures terminate.
The existing Python-compatible Rust logging producer records actual Rust
file/function/line, OS PID/thread, source commit and runtime-language context;
its exception detail is a real Rust parse error, never a fabricated Python
traceback. The existing collector publishes it to logMessage/errorLogMessage
and writes its ordinary disk record.

## Child ownership

The Rust process polls its owned pipe, preserving fragmented UTF-8 and final
unterminated lines, and observes SIGINT/SIGTERM while idle. Every ordinary exit
path sends SIGTERM to the owned child when needed and waits for it. It ignores
the child's exit status just as the source does. It does not add an inference
clock, retry policy, child timeout or forced-kill escalation; an external child
that ignores termination can still block wait, as in the original implementation.

The actual original main reproduces an inherited direct-SIGTERM defect:
parent exit -15 leaves its child alive because Python's default signal action
does not execute `finally`. [#66](https://github.com/bin9208/openpilot-rust/issues/66)
tracks this separately. The native process reaps its child on SIGTERM. Original
SIGINT does clean up, and the manager normally catches its KeyboardInterrupt;
the direct-source test exits -2 whereas native orderly shutdown exits 0. These
facts do not establish a leak in ordinary manager SIGINT shutdown.

## Executed evidence

The isolated worktree's `.omo/evidence/journald/INDEX.json` records exact commands,
revisions, binary hashes, observable assertions, raw IPC packets and child PID /
PPID / signal traces. The checker executes the unchanged source file directly;
it does not replace its main function or manufacture source provenance.

A PATH-selected synthetic `journalctl` validates the exact argv and forwards
controlled bytes through a real OS pipe. Readiness comes from observed child
ownership and real queue creation/subscriber handshakes. No stderr readiness
marker, host journal read, real device connection or device deployment is used.
Both bridges feed original msgq/cereal receivers and the original log collector.

The host, ASan (including native C++ transport instrumentation), and generic
AArch64 GNU/QEMU scenarios each compare:

- 155 exact ordered messages, including 128 fixed-seed float/wide-integer records,
  Unicode/surrogate/duplicate-key/default/range cases, a 5000-level record,
  fragmented UTF-8, CR/CRLF,
  empty lines and an EOF-terminated final record;
- 13 recoverable syntax failures, with 13 ERROR publications on each logging
  topic and 13 actual collector disk records;
- 19 fatal root/conversion/range/UTF-8 cases with no subsequent publication and
  observed child termination/reaping;
- idle SIGINT/SIGTERM, stdout EOF while the child remains alive, and an external
  child exit status 17, with source/native exit differences recorded above.

Six Rust regression tests lock message presence, exact ordered JSON, integer
conversions, syntax-versus-fatal classification, the integer resource limit and
stack-independent handling of source-valid deep records.
Initial comparison failures and their repairs remain captured: source harness
import/default-pointer assumptions, byte-string literal-control rejection and
integer control-whitespace rejection, and removal of a provisional nesting cutoff. The final ledger distinguishes those
failed attempts from completed checks.

Run the source/native scenario with an isolated original msgq binding:

```sh
python rust/tools/build_msgq_python.py --output <native-python>
PYTHONPATH=<native-python>:.:rust/tools python rust/tools/check_journald.py \
  --binary rust/target/debug/journald-rs --output <evidence-directory>
```

The checker declares its pinned CPython 3.12 dependencies. The build helper also
needs Cython and setuptools. Omitting `--binary` executes only the source oracle.
For ARM, pass an executable wrapper that invokes the built GNU AArch64 binary
under `qemu-aarch64-static` and its matching userspace sysroot. That validates
emulated process/pipe/IPC boundaries, not AGNOS, physical journal behavior,
device performance, complete runtime startup or user device acceptance.

Production daemon selection is unchanged. Exact-commit integration Actions and
later full-runtime manager/device acceptance remain separate delivery gates.

Docs-Not-Needed: this internal daemon port adds no user setting, CLI contract or
user-visible behavior change to the selected production runtime.
