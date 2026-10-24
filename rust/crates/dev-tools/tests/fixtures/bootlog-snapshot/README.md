# Original bootlog snapshot contracts

`original.json` captures the unchanged Python checker/source adapter using the real
original Cython Params binding, real threads, subprocesses, files, chmod/mtime,
xattrs, symlinks, FIFO and `/dev/null`. The ten source outcomes were retained
before native-checker implementation from the original/native #353 comparison.
Each original invocation and all live source hashes remain in the corpus. Its
whole SHA is `af18f39ad3e83c9c823a4beca7917a32c69452a172dfc465e91b6b91d50a6743`.

`process-original.json` independently executes original `line(process)` over six
real pipes: EOF, malformed JSON, null, array, CRLF and invalid UTF-8. Whole SHA:
`b5d3e52110403ccaaabb23bac066cc03b0c525c8d68e11302cef5d431c4aefed`.

`params_path.cc` has two mechanical boundaries. Path mode constructs unchanged
C++ Params and returns getParamPath. Call mode forks, closes nonstandard
descriptors, execs the specified executable, waits and reports actual exec errno
and child status. It contains no snapshot, copy, test-case or expected-result
policy. The source-derived Rust reference implements save_bootlog, copytree and
copy2 independently of openpilot-bootlog. The child reads actual metadata and
bytes, waits for the checker to mutate the live Params file, then observes both
the retained snapshot and live file.

The required source-reference test compares every actual result to the fixed
independent original expectation. Production validation separately compares that
live source reference against the supplied native snapshot example. Both must
pass before original entrypoint retirement. The generic Params binding loader
has other consumers and is retained.

These are host filesystem/process contracts, not device startup, upload,
allocation-count or CPU measurements. Native build, seven required tests and strict Clippy passed. Both final baseline
and inherited BOOTLOG_SPECIAL_COPY=1 runs passed all ten cases against the
current native example. Each used a Python-free PATH; actual exec traces record
61 exec attempts and zero Python/PyPy executions. Forty invocation artifacts
retain the original string-array schema. A separate provenance receipt identifies
the executing checker, source reference, observation child, candidate, Params
helper and launcher by exact ELF SHA256. Parent post-retirement validation and
required remote CI remain separate gates.


The original ambient-environment full checker capture is preserved in
ambient-original.json (whole SHA
3e390daa4198437ac3c274585234a0a03613bbb790cafed4712fc34d01b59be2).
It proved that equal live source/native observations remain valid when inherited
child flags differ from the capture machine. The draft's extra production
baseline comparison was observed RED and removed; immutable original baseline
expectations remain mandatory in independent source-reference tests.
output-original.json independently captures refusal of an existing empty case
directory. The native exclusive-create regression was observed RED then fixed.

The process adapter's descendant regression likewise failed before repair:
reaping a successful leader early lost its reserved process-group identity.
It now observes exit with WNOWAIT, terminates descendants while the leader still
reserves that identity, and reaps in its owner. The original fifteen/five second
protocol bounds, raw metadata and filesystem observations remain unchanged.

Detailed ignored evidence: 2026-10-18-bootlog-snapshot-check/final-proof.json,
final-{build,tests,clippy}.log, final-sources-unchanged.log and
final-{baseline,ambient}.execve. Both replaced Python entrypoints were removed
after those gates. Parent post-deletion ten-case comparison and the combined
67-test journal/bootlog/CI suite passed; remote CI remains a separate gate.
