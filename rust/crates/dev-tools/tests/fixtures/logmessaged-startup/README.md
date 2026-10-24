# Logmessaged startup controller contracts

`original-observation.json` retains independent unchanged-Python observations
from [run 39192427750, support-runtime job 117868253013](https://github.com/bin9208/openpilot-rust/actions/runs/39192427750/job/117868253013),
source commit `11f3dec411662baa961733b363c0b38d7082fcfa`. It records the artifact
ID and archive SHA256, original report SHA256, all three observed payloads and
their original packet hashes. It contains no candidate output. The archive did
not retain private collector swaglog files, so live native disk records remain
checked against the original source contract, not falsely labelled as captured
original disk bytes. The existing live original/native full disk-row gate now
also writes `original-disk-rows.json` and a hash/provenance manifest to the
visible proof root. Its next successful Actions artifact must be inspected and
accepted before replacing that live gate.

`source-pins.json` pins the historical Python checker, its ftruncate pause fixture,
and the retained independent collector-source inputs. Only the exact historical
checker may be absent; a present changed checker, symlink, FIFO or directory is
rejected. Its digest is reported as historical provenance, outside live input
hashes. The new checker also
binds the actual executing ELF and the authored source boundary compiled into it.
No Python helper is executed by the native implementation.

The original checker has three ordered cases:

| Case | Collector | Refresh after publisher reset | logMessage | errorLogMessage |
|---|---|---|---:|---:|
| legacy-source | independent source | no | 1 | 0 |
| ready-source | independent source | actual debug ACK and error reader receive | 1 | 1 |
| ready-native | supplied production executable | actual debug ACK and error reader receive | 1 | 1 |

Every publication must contain the exact first-error JSON string, with valid
full-schema Event and correct union. Each case writes exactly one disk JSON
record whose `msg$s` is `first-error-after-start`. The original 200 ms delay,
1-second observation loop, 5 ms poll cadence, 20-second startup/readiness and
10-second stop waits are retained. Invalid, extra and missing records fail.

The native source worker reuses the independently ported collector policy. Its
short private Unix gate deliberately constructs the error reader before the
error publisher; parent subscribers attach before that publisher resets the
queue. The unsynchronized legacy case therefore still observes the actual
reader invalidation. This mechanical schedule must be validated against the
unchanged original C pause in CI; source-only preparation is not that proof.

No C compilation is performed by the new checker: `compiler` is an empty list,
compiler stdout/stderr files are empty, and `initialization_gate` describes the
actual native reset handshake. The original fixture SHA is retained as historic
policy provenance; executing source/checker/candidate ELF hashes are separate.
This deliberate tooling metadata change must not be presented as an original
compiler execution. Cases, observed payloads, disk assertions and source hashes
remain comparable without changing the originals.

Ownership uses the existing group supervisor and reserved child process owner.
A live authenticated endpoint acknowledgement reserves its filesystem inode
before candidate observations; foreign replacement endpoints are preserved.
The bind-to-ACK abrupt-death window is conservatively not cleanup-owned. Final
stdout is fallible and precedes atomic create-new receipt publication, with a
held-file identity for cancellation cleanup.

Both `logmessaged_native.py` and `logmessaged_reference.py` remain mandatory
shared inputs because other Python controllers still consume them. The startup
checker remains in the tree until the new Actions artifact captures the missing
disk rows and the parent updates its caller and CI gate; deletion follows
exact-SHA cloud proof.

**Validation status:** the independent original/cloud pair passed on the pinned
run above. The retirement changes are source-only pending a new exact-SHA cloud
run of the absent-path, reappearance, provenance, native wire, Clippy and
no-Python execution checks. No local build, runtime or probe was executed.
