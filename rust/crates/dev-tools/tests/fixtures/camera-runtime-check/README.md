# Independent camera runtime checker inputs

These fixtures precede native implementation. Original source SHA256/provenance
are recorded in verify-original.json, controller-original.json and cli-original.json.

Eight deterministic gzip files retain complete earlier original live captures:
trace rows, message wires/pycapnp dictionaries, VisionIPC metadata, logs, run
receipt, original report and actual C++ state-oracle replies. No sampling or
candidate-generated expected values is used. Compressed/uncompressed digests and
sizes are recorded in the verifier manifest.

The unchanged original verifier was exercised 44 times, including all eight
successful cases. Real source cleanup audit and pycapnp decoding remained active;
only the source-oracle process boundary replayed earlier actual C++ replies.
Captures bind exact generated artifacts, requests, failure phase and stdout.

The unchanged original controller was separately exercised 24 times with
controlled process, clock, Msgq, VisionIPC and logging boundaries. Real pycapnp
decoded source-captured wires. This covers all eight cases, QEMU/environment,
signals/publication, startup/connect/runtime failures and final cleanup. Its
verifier is independently covered rather than replaced in native production.
Capture does not touch ambient IPC namespaces. Twelve additional actual original
CLI subprocesses capture admission/side effects.

Original setup failures before its try/finally can leak their namespace. Native
owned cleanup explicitly improves those three early failures while preserving
the original failure phase and artifact contents. Device checks remain user-owned.

Step-one verifier test preparation adds 44 compressed original input deltas under
inputs/, independently captured before native implementation. Original expected
verify-original.json bytes remain unchanged. Five schema fixtures are exact
copies of source files already used by the original live captures; their source
paths/digests are recorded in schema-original.json. The prepared Rust test loads
the actual NativeSchemaDecoder, preserving real schema/wire validation.

Initial native verifier gate passed all three tests: 44 source contracts with
exact oracle calls, stdout and partial artifacts, plus full corpus/schema pins.
The test replay took 223.91 seconds and strict scoped Clippy passed. This covers
the artifact verifier only; native controller/live IPC retirement remains pending.

Further independent captures preserve eight verifier coercion controls, one
controller receive-error control, and malformed batch decoding. All earlier
expectations remain unchanged. The receive-error case retains a successfully
decoded message before a later socket failure. The batch fixture records actual
pycapnp admission of a valid frame and rejection of an out-of-bounds root pointer;
external Cap'n Proto emits partial JSON and exits nonzero, so native batch decoding
falls back to the original per-row path and stops at its first error.

Native evidence on 2026-10-18: four verifier tests cover all 52 original contracts;
three controller tests cover all 25 protocols; five native tests cover real
Msgq/VisionIPC/ZMQ, inherited stdin, live/exited child ownership (Drop, explicit
wait, descendant termination before reaping, and rejection after reaping), original CLI
admission, and independent schema batch projection/failure order. Strict scoped
Clippy passes. An initial live native timeout was reproduced alongside a fresh
successful original run under the same loader environment. Finite raw-message
snapshots and one independent Cap'n Proto conversion per batch resolved it without
changing the ten-second runtime deadline or any original assertion.

All eight host runtime cases subsequently passed with Python absent from PATH,
using the existing isolated #296 camera binary, real C++ oracle and preload fixture.
Both original and native verifiers accepted every captured run, with byte-identical
report, stdout, cleanup audit and all oracle artifacts per run. This is host
comparison evidence; a freshly rebuilt parent orchestrator gate and post-deletion
proof remain required before final retirement. No device run is claimed.

Source review also captured eight additional log-type controls independently from
the original: four prefix/membership failures, dictionary/list membership positives,
and irrelevant/trailing JSON values that must short-circuit without reading a
missing level. Both original_log_* tests failed on the uncorrected native checker, then passed
all eight cases after raw-value equality, codepoint prefix checks and Python
membership semantics were restored. The bounded regression gate took 23.68
seconds and strict scoped Clippy passed in 5.68 seconds. Earlier corpora remain
unchanged; the full verifier target now has six tests (60 original contracts).

Final retirement evidence: the complete native CI orchestrator passed58 live
commands with37 executable hashes unchanged. After deleting the three original
runtime/cleanup/orchestrator Python files,49 focused tests and strict Clippy pass.
A fresh current-source camera daemon passes all eight host runtime cases against
the freshly generated original C++ fixture/oracle, with PATH=/nonexistent.
The CI job now requires the native orchestrator. Physical device acceptance and
remaining Python dependencies elsewhere in the job/repository remain separate.
