# Independent dynamic Params interoperability checker contracts

`original.json` captures the unchanged complete `rust/tools/check_params_reference.py`
with controlled compiler/store process replies. Python 3.12.15 executes the original
copy/build/check control flow. Input hashes and lengths represent exact binary stdin,
including 262,144 bytes spanning every byte value; the full capture remains ignored
in `.analysis/scratch/2026-10-16-params-reference/`. Expected values are not derived
from the Rust checker. Schema and source files are copied/read dynamically.

The 22 captured whole-checker cases preserve compiler command order, catalog comparison, binary
and empty-value interoperability, remove/clear/unknown-key behavior, inherited
compiler streams and store stderr, raw store stdout, child failure phases, both
lock checks, post-release content assertions and temporary-directory cleanup.
Schema glob order is unsorted filesystem order; only the schema argument tail is
adapted to the executing filesystem while preserving its multiset and fixed slots.
The C++ Params implementation remains a fresh dynamic oracle, not a frozen catalog.

The original Popen context can wait indefinitely after its ten-second release
wait raises TimeoutExpired. A separately owned controlled process was still blocked
at an external twelve-second harness deadline and was then killed/reaped; this is
not an original/native exit-status parity claim. Native cleanup explicitly retains
the 0.1-second blocked probe and ten-second post-unlock wait, then kills and reaps
only its owned child on failure/timeout. Ordinary compiler/store operations do not
acquire a new timeout. Real lock-peer coverage exercises advisory locking and
owned-process cleanup independently of the orchestration process mock.

The final complete original/native dynamic C++/Rust store gates pass all 367
keys for normal paths and a raw-byte shell candidate plus raw-byte Cap’n Proto
prefix. Complete stdout/stderr match and each isolated temporary parent is empty
afterward. The native PATH has only capnp/capnpc-c++/g++/as/ld, with no Python.
`actual-proof.json` records source provenance, binary hashes and results. Eight
integration tests, two cleanup units and strict focused Clippy pass. This host
checker does not establish vehicle or ARM acceptance.

Independent copy captures also cover ten pre-compiler failures, including FIFO
rejection and a private source symlink to `/proc/self/mem` that fails its first
read and leaves an empty target. Copyfile opens source, then target, then copies;
copytree additionally copies time, xattrs and mode, follows source links and
continues sibling copies before reporting errors. The child-only umask0444 proof
requires path-based timestamp updates without destination read permission.

Readonly owned-directory cleanup is tested on both success and body failure.
Cleanup failure must propagate and replace an earlier body error, matching the
original TemporaryDirectory primitive. Native cleanup never follows symlinks or
repairs permissions outside its owned tree. Temporary selection tries nonempty
TMPDIR/TEMP/TMP in that order, then /tmp, /var/tmp, /usr/tmp and current directory;
it probes file creation/write/removal before selecting a directory. Actual
original fallback captures distinguish failures before compilation from the
expected compiler failure after successful fallback. Ten argument/help captures
also retain parser exit codes and absence of temporary artifacts.

The whole-original cleanup-failure capture retains the successful PASS line before
raising PermissionError and leaving its temporary tree. The writer-based native
API preserves that print-before-cleanup phase, including nonzero final status.
A separately captured remove operation replacing its key with a self-symlink
also preserves Path.exists' ignored NOENT/NOTDIR/BADF/LOOP errno semantics rather
than rejecting an originally accepted case.
