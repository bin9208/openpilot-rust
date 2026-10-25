Original independent capture: unmodified check_selfdrive_params.py ran all 47
cases using the real unchanged CPython 3.12.15 Cython Params binding, real
filesystem stores, actual ZMQ PUSH/PULL logging and seven SIGABRT std::stoi
readers. The expected fixture copies only original source values, warning
messages and fatal results. Candidate results never supply expected values.
Volatile raw log metadata, binding and original output remain in ignored scratch.
The compact fixture is 819 bytes gzip; manifest pins sources and dependencies.

The native checker uses a separately built narrow original C++ Params entrypoint
on live files. Its long-lived source side reads text bytes/getBool/getInt and
retains uncaught getInt fatal exceptions in separate processes. Native UTF-8
conversion and Python byte repr supply the pinned Cython STRING warning boundary.
Actual PUSH/PULL warning and marker packets are sent, received and severity
checked; the native candidate stays alive across all 47 requests. Every dynamic
source value/message is also checked against the independent Cython capture.

Intentional interface/provenance delta: --reference replaces --binding. The
report uses reference_sha256 plus explicit source-reference provenance instead
of claiming Cython was executed. Source log metadata identifies this native
reference (PID/time are fresh); packet payload messages and barrier semantics
retain the original comparison. Source-reference Cython adapter failure/cleanup
is represented by native errors, never by executing a Python wrapper. The seven
fatal stderr artifacts and final stdout are retained. The original generic
Params, logging sources and other fixed checkers remain live.

Registered and compiled with the optional selfdrive-params-native feature.
Nine focused unit tests and five integration tests passed; the separate unchanged
C++ source helper built successfully. The actual native candidate matched all 47
source cases, seven fatal diagnostics, six warnings and stdout under PATH=/nonexistent.
All 15 original boundary outcomes, partial artifacts and warning order matched.
A copied source root with only the retired checker absent also passed all 47.
The durable Rust matrix test and controlled Rust peer are registered and passed.
Strict Clippy passed for the library, checker, builder, peer and integration target.
CI migration and actual-root source retirement remain parent-owned. No device is needed.

Independent boundary capture in boundaries.json additionally runs unmodified
original main with controlled reply peers, real Cython Params and actual ZMQ:
wrong/EOF readiness, invalid JSON/value, missing/incorrect log severity, incorrect
warning level, nonzero final exit, float/boolean severity equality, escaped and
raw surrogate metadata, UTF-16 packet bytes, CRLF lines and final response without
a newline. Each case records stdout, the exact partial artifact set and ordered
source warnings. All original temporary stores were removed. Scratch
capture_boundaries.py regenerates these without invoking the native checker;
prove_native.py compares the live native execution against both captures.

The independent manifest bytes have a separately recorded SHA-256 in the checker;
dropping any source pin cannot silently narrow coverage. Live source drift is
checked every invocation. Generic source files remain required; only the retired
fixed checker may be absent, and a reintroduced changed checker is rejected.

Explicit failure-control delta: original TextIOWrapper pipe reads/writes could
block indefinitely; the native child owner bounds readiness/response/write/exit
to five seconds and kills its owned process group on any failure. The direct
leader remains unreaped until group termination to prevent process-group reuse.
Core dumps are disabled in children as in original main. Child, sockets and
temporary store close before a success report is published. Report row order
retains original fields, with the documented reference provenance substitution.

Dependency boundary: selfdrive-params-native enables existing pinned zmq 0.10.0;
its checker, builder, controlled test peer and integration test require that opt-in feature. The default
dev-tools graph does not add libzmq compilation. No unsafe code is added to
dev-tools and the workspace forbid(unsafe_code) remains unchanged.

The Rust matrix regression builds the actual unchanged C++ reference, then runs
normal plus all 15 captured boundary modes using selfdrive-params-test-peer.
That controlled Rust candidate is a test fixture, not an oracle or production
Params implementation. It injects captured values/messages only to test checker
admission and failure side effects. Real candidate differential evidence above
uses the existing alert_params executable on live filesystem state.

QA helper-only execution control: PR_SET_DUMPABLE=0 avoids the host kernel
pipe core_pattern invoking a system apport/Python collector despite RLIMIT_CORE=0.
The unchanged Params getInt still throws and terminates with SIGABRT; all seven
return codes and diagnostic bytes are checked against the original Cython capture.
This is not a production runtime or Params policy change.

Child ownership regression explicitly checks both successful EOF/finish and
error Drop: the owned group kills a background descendant, the direct leader is
reaped, and subsequent waitid reports ECHILD. Termination of nonchild descendants
is asynchronous; a stopped zombie may await its own parent/reaper. The test
checks that no descendant remains running, rather than claiming to reap nonchildren.

Final local proof uses alert_params freshly built from the current #296 worktree
into cargo-target-retirement-296 (SHA-256
38b19d661a2ecdc269f7e0aeb690d4dd3000bcd0c49880977842744878d2c3cd).
This supersedes earlier exploratory runs against the shared-cache example.
The fresh candidate passed all 47 cases/7 fatal diagnostics/6 warnings plus all
15 boundary outcomes under PATH=/nonexistent, and passed again against a copied
root with only the fixed checker absent (22 generic source pins retained).

Final gates: nine unit tests, five integration tests including the actual C++
reference/native Rust peer matrix, and strict Clippy all passed. The matrix took
9.06 seconds after suppressing external core processing (76.09 seconds before,
with identical captured values/errors/artifacts). CI isolation 33, disk bootstrap
13 and workflow policy 6 tests passed after the intended missing-route RED.
No original source deletion or Git mutation was performed by this implementation
worker; actual-root retirement/integration remains parent-owned.
