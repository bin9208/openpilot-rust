# Independent boot artifact checker captures

Only synthetic inputs and source-derived JSON belong here. Actual executable,
zstd and Cap'n Proto artifacts remain ignored in the owned scratch directory.

The unchanged check_bootlog.py was freshly executed against the retained,
source-matched C++ bootlog and the Rust bootlog: all16 base cases passed. A fresh
controlled build of unchanged bootlog_output_fault.c then passed all four fault
cases. cases-original.json captures all20 definitions directly through unchanged
original main; artifacts-original.json and faults-original.json contain all20
actual original result objects, including full encoded event dictionaries.

upload-original.json captures21 actual filesystem selections through unchanged
check_uploader.source, covering locks, attributes, uploaded entries, directories,
links, priority, segment ordering, Unicode and metered requested-route admission.
This unit retains the shared check_uploader.py for its other consumers. The
replacement must implement selection independently, without importing candidate
uploader code or reading these expected rows at runtime.

wire-original.json contains two actual original zstd streams and their complete
raw Cap'n Proto bytes as synthetic hex, alongside unchanged full-schema pycapnp
projection. Ordinary and NUL journal cases preserve all fields and embedded bytes.

process-original.json records five real child/failure cases: source command,
native command with runner, absent binary, non-executable binary and preexisting
output. Child-observed argv, cwd and zero core limit, exact stderr, exit status,
invocation fields and artifact creation phases are retained. Dynamic output paths
use explicit placeholders; tests independently check the actual namespace digest.
cli-original.json records nine genuine unchanged CLI help/error invocations.

journal-exits-original.json adds13 actual original shell exit cases. Nine initial
native helper disagreements were independently reproduced before repair. The
helper now execs the same immutable system shell with original exit syntax and
diagnostic location, preserving integer limits and signal/PID semantics.

command-output-original.json records four original command processes, including
NUL, invalid UTF-8 and unterminated raw bytes. Three frozen native helper failures
exposed a missing stdout flush before exec. Both implementations had received the
same truncated input, so their20-case pair comparison alone passed incorrectly.
The independent original baseline caught the missing final bytes. An explicit
flush and a mandatory actual-helper regression preserve all original bytes.

Final verification: nine artifact tests and strict Clippy pass; the combined CI
policy/timed-fixture gate passed58 tests before the final flush regression was
added. A frozen repaired checker/C++ reference/Rust candidate/fault library then
passed all20 cases with an empty PATH. Both sides' complete normalized result
objects equal all20 unchanged-original baseline objects, without dropping any
additional fields. The196 execve calls contain no Python and leave no processes.
Sources and four frozen ELF/library identities remain unchanged. Exact receipts,
trace and audit live under the owned scratch directory as frozen-repaired/receipt.json,
repaired-native.execve and repaired-proof-summary.json. CLI admission tests retain
original success/error status and stream selection; native help text names the
replacement executable rather than claiming byte-identical Python usage text.

Capture scripts and raw evidence:
.analysis/scratch/2026-10-18-bootlog-artifacts/.
