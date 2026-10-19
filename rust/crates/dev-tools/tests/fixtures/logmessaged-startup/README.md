# Logmessaged startup controller contracts

This is a source-derived contract, **not a captured native answer corpus**.
`source-pins.json` pins the unchanged Python checker, its ftruncate pause fixture,
and all nine retained independent collector-source inputs. The new checker also
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
shared inputs because other Python controllers still consume them. The original
startup checker is also retained until reviewed CI original/native comparison
passes. Nothing has been renamed into a fixture or claimed retired here.

**Validation status:** new implementation and tests are source-only. Formatting
and diff checks may run locally; compilation, missing/API admission checks,
all eight controller tests, the wire unit, the mandatory Rust paired-reference
audit, Clippy and unchanged-original/native proof are CI-owned under
the user workflow override. No local build, runtime or capture was executed.
