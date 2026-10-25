Independent original timed QA captures taken before native implementation.

`reference-original.json` preserves all 54 scenario inputs from the unchanged
`check_timed_reference.main` enumeration. The native comparison arm was replaced
only in the scratch capture harness; actual `timed_reference.Source` executes
unchanged source definitions and real safe sudo/Params/filesystem operations.
All returned values, logs, sleeps, last attempt, publications and errors remain.
Temporary root/prefix names alone become `<root>` and `<prefix>`.

`dates-original.json` preserves every one of the original 360 inputs across five
timezones. CPython local datetime/fold errors are captured as exact type/message,
along with sleeps, commands, Params and log state. No candidate Rust timed code
or output supplies an expectation.

Live HTTP/IPC captures use existing original IPC extensions and unchanged
Python workers, with exclusively owned namespaces and loopback HTTP:

- `http-original.json`: 22 modes, including real progress over five seconds,
  per-read timeouts, duplicate keys, nonfinite unused fields and redirects.
- `logging-original.json`: two real closed-ZMQ propagation and Params ordering
  cases. Only the native branch/cross-arm equality is omitted in the capture
  harness; every original source-side assertion remains.
- `daemon-original.json`: both GPS topics, five actual clock packets per case,
  real ten-second sleep, original logs and safe date command.
- `date-daemon-original.json`: all eight original source extreme-date workers.
- `shutdown-original.json`: original SIGINT during observed GPS sleep and
  progressing local proxy HTTP; actual peer closure and elapsed bounds checked.
- `command-original.json`: nine safe fixture child exit/record/side-effect cases.
- `cli-original.json`: 40 argparse controls across all eight checker entrypoints.

Variable clock/syscall/host/process values remain in ignored raw captures and
are omitted here only after their original field/duration assertions run.
Current source and dependency hashes accompany every corpus. The unchanged
original sleep-readiness pytest also passed (one test). The seven production
lifecycle checks were subsequently captured against a fresh current-source native
daemon in `lifecycle-original.json`. The complete native live proof subsequently
passed before and after retirement, as recorded below.

The shared `logging_producer_reference.py` remains required by other tool
families. The thirteen family-specific Python sources were retired after full live proof;
this shared producer and twelve live-source dependency pins remain required.

Scratch capture scripts and coordination ledger:
`.analysis/scratch/2026-10-18-timed-tools/`.

Additional unchanged-original boundary captures, kept separate from the initial
corpora:

- `command-extra-original.json`: ten component-containment, root normalization,
  argument-arity and failure-class controls (19 fixture cases in total).
- `actions-original.json`: 15 signed-zero, nonfinite and arbitrary-integer policy
  actions, including exact command integers and error-before-log boundaries.
- `native-original.json`: 16 candidate process, decode, JSON, output-write and
  timeout boundaries; `native-logs-original.json`: eight actual PUSH transport,
  JSON encoding and log-publication boundaries.
- `proxy-original.json`: four real loopback urllib proxy/bypass cases.
- `checker-failures-original.json`: 19 original checker failure/artifact cases;
  `checker-prefix-original.json`: exact ordered reports and six artifacts from
  one original two-child scenario.
- `http-report-original.json`: stable original nested request ordering.
- `command-publication-original.json`: complete original JSON command-line
  visibility at a controlled write scheduling point (actual unchanged helper).
- `lifecycle-original.json`: seven original production-entrypoint checks against
  the current isolated-worktree native daemon, including CLI failures.
- `live-worker-original.json`: seven date-worker CLI integer admission and
  post-initial-clock schema-overflow boundaries. Unicode signs/decimal digits,
  underscores and whitespace remain accepted; Python integer admission is
  distinct from the later real GPS schema assignment.
- `live-cli-extra-original.json`: 18 nargs/help/abbreviation/repeated-worker
  controls, supplementing the original 40 checker CLI captures. CPython 3.12.15
  integer `nargs` consumes option-looking strings in a complete worker tuple.

The first four native checker implementations have now been exercised against
fresh, unchanged original invocations and the same current-source candidate:
54 reference scenarios, 360 datetime cases, 44 HTTP source/native workers, and
two logging cases passed. All 416 reference/date summary/comparison artifacts
match byte for byte. HTTP reports match with only independently validated elapsed
fields removed; stable nested request key order remains checked. Logging summary
bytes match. Frozen native runs used `PATH=/nonexistent`; every observed execve
attempt was inspected and no Python execution occurred. The earlier report-order
and command-publication failures remain in ignored scratch alongside the repaired
proof. The final proof additionally covers all four live controllers: daemon4,
date-daemon16, lifecycle7 and shutdown6, for493 cases across all eight checkers.
The same frozen eleven QA tools and exact two original candidate ELFs passed
before and after deletion; stable build metadata was not normalized. Each run
audited1,025 execve attempts with zero Python and27 invocation-bound ownership
receipts. Post-deletion118tests and strict Clippy passed. Original Git blobs and
raw output hashes remain immutable provenance; no retired source was restored
or executed after deletion.

Ownership receipts in the live worker use complete-file atomic replacement, so
interruption during later metadata updates cannot truncate the preceding receipt.
The interval between creating a new IPC object and publishing its first ownership
flag remains a bounded crash window; this is not a universal crash-cleanup claim.
Native reference logs identify their actual Rust implementation. Historical
`implementation="python"` report labels remain for compatibility and do not claim
that Python/Cython executed in the native run.

`lifecycle-stdin-original.json` records an additional unchanged-original lifecycle
probe: an immutable launcher reads inherited stdin and then execs the same real
current-source daemon. The original passes the token to the first of seven
launches; later launches observe EOF. The first native implementation lost that
token despite passing its seven normal entrypoint checks. Its actual RED receipt
and ELF hashes are retained in scratch. The regression requires an explicitly
built `TIMED_NATIVE_ENTRY`, with no fallback to another checkout or unavailable
skip. The tracked launcher is executable and is never rewritten during tests.

`support-ci-original.json` records the pre-migration support job's 46 unrelated
shell lines, environment, timeout and native package prerequisites. The native
routing regression retains these while adding all eight checker invocations,
three helper binaries, all 16 focused suites, strict Clippy and fresh disk guards.
This is a CI preservation contract, not a runtime output oracle. Its routing RED
and subsequent workflow migration are still pending.

`hostname-original.json` preserves two real original source/PUSH observations with
`HOSTNAME` absent or spoofed. Original `SwagFormatter` freezes the actual kernel
hostname. The native reference initially used the environment variable and failed
the full controller report comparison despite all four commands exiting zero.
That RED is retained. The new regression compares actual record host with an
independent kernel observation, not the capture machine's literal hostname;
the repaired full controller replay remains pending.
