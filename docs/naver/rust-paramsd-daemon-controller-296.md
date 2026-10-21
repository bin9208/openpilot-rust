# Native paramsd daemon controller preparation (#296)

The unchanged `rust/tools/check_paramsd_daemon.py` is still the independent
source controller and comparator. It generates both input cases, runs the
original Python source trace, then drives the native daemon. Its continuous
case contains one empty startup frame plus 1,202 four-packet frames; restart
contains one empty frame plus eight. The original checker must run to success
first against the exact daemon ELF and oracle used by the native lane. Retain
its complete `continuous/` and `restart/` evidence, especially `input.json`,
`source.json`, `native.json`, `packets.json`, `saved.bin`, and `results.json`.

`check-paramsd-daemon-native` consumes the two **original-produced**
`input.json` files. It validates their case names, frame counts, first empty
frame, ordered services, and flags before creating a fresh msgq/Params
namespace. It uses the already-reviewed `StartupGate::attach` to send only
the two phase descriptors to the selected daemon, verifies the held second
poll after exactly one completed empty update, then publishes all 1,202 and
eight batches through native `PubMaster`/`Subscriber`. It waits for the
daemon's actual reader registrations, requires an exactly committed livePose
cursor before each gate release, and captures every raw liveParameters packet.
The controller checks packet timestamps, the continuous saved packet at index
1199, restart preservation of the stored saved payload, exact GPS bytes, and the
original INT/TERM five-second shutdown contracts. Process-group cleanup
kills and reaps its owned daemon if any check fails. It records source-facing
`packets.json`, `saved.bin`, `gps.json`, `poll.jsonl`, stdout/stderr, maps,
reader readiness, and `result.json` for both phases.
Each received raw packet is also flushed immediately to `packets.jsonl`, so
failure on a later batch retains the complete prefix of actual publications.
Restart publications have new `logMonoTime` values; the restart check requires
the stored `LiveParametersV2` bytes to remain unchanged, and leaves publication
semantics to the independent decoded source comparison.
The native controller installs scoped SIGINT/SIGTERM flag handlers. Cancellation
is observed within each startup wait, pose cursor wait, publication receive,
and shutdown wait; returning through the daemon owner kills its separate
process group and reaps it. The existing total 10-second gate, two-second
cursor, five-second publication and five-second shutdown limits are retained.

This is a native **runtime controller**, not a substitute source oracle.
The leaf `paramsd_daemon_reference` test independently decodes its raw packet
arrays with the pinned Cereal schema and compares all 1,210 events against the
corresponding original `source.json` rows using the unchanged
`check_paramsd.compare` numeric tolerance and path policy, excluding only
`logMonoTime` as the existing checker does. It checks every JSONL raw packet
against the final array, exact input and binary hashes, source gate reports,
publication counts, saved state within each lane, GPS, signal and exit. The
unchanged Python checker continues to compare its own live result to the
source separately. Do not compare timestamp-bearing packet bytes
directly: each run has its own monotonic clock. Do not retire either Python
file until the source-paired check and a later same-ELF absent-source execution
audit pass.

Registered Cargo entrypoints:

```toml
paramsd-daemon-native = ["paramsd-startup-gate-native", "dep:capnp", "dep:openpilot-cereal", "dep:openpilot-messaging", "openpilot-messaging/native", "dep:openpilot-msgq", "openpilot-msgq/native-skip-miri"]
[[bin]]
name = "check-paramsd-daemon-native"
path = "src/bin/check_paramsd_daemon_native.rs"
required-features = ["paramsd-daemon-native"]
[[test]]
name = "paramsd_daemon_check"
required-features = ["paramsd-daemon-native"]
[[test]]
name = "paramsd_daemon_reference"
required-features = ["paramsd-daemon-native"]
```

No new dependency is required; all four optional libraries already exist in
`openpilot-dev-tools/Cargo.toml`. The controller binary and process test
include the same `paramsd_daemon_check.rs` source without changing shared
`lib.rs`; the reference test is independent of the controller's decoder.

After the original checker completes and its source/inputs and daemon/hook
ELFs are sealed, run the native controller with fresh **absent** evidence
directory and absolute paths:

```sh
check-paramsd-daemon-native \
  --daemon "$PWD/rust/target/debug/openpilot-paramsd" \
  --library "$RUNNER_TEMP/estimation/paramsd/daemon/continuous/startup-phase.so" \
  --continuous-input "$RUNNER_TEMP/estimation/paramsd/daemon/continuous/input.json" \
  --restart-input "$RUNNER_TEMP/estimation/paramsd/daemon/restart/input.json" \
  --evidence "$RUNNER_TEMP/estimation/paramsd/native-controller"

PARAMSD_ORIGINAL_OUTPUT="$RUNNER_TEMP/estimation/paramsd/daemon" \
PARAMSD_NATIVE_OUTPUT="$RUNNER_TEMP/estimation/paramsd/native-controller" \
PARAMSD_NATIVE_DAEMON="$PWD/rust/target/debug/openpilot-paramsd" \
PARAMSD_NATIVE_HOOK="$RUNNER_TEMP/estimation/paramsd/daemon/continuous/startup-phase.so" \
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools \
  --features paramsd-daemon-native --test paramsd_daemon_reference --locked
```

The copied hook must have the exact hash of the source-built shared object
used by the original lane. Parent CI must seal source, input and binary identities before
and after both lanes and retain actual original/native outputs. The ignored
`full_continuous_and_restart_native_controller` test is an alternate cloud
entrypoint using the five `PARAMSD_CONTROLLER_*` absolute-path variables;
running the binary once is enough for the full process gate. The unignored
test pins the unchanged original checker, fixture generator and C hook.

All code and assertions here are source preparation. No local daemon,
original Python, build, or functional test was executed.
Parent-owned source-policy registrations likely affected are
`ci_rust_isolation.rs` (new required exact source/native comparison step) and
`ci_disk_bootstrap.rs` (new guarded Cargo test marker). Keep the existing
original checker, atomic gate test and five original pytest cases.

## Native success receipt requires sealed original agreement (2026-10-21)

The native controller now calls a fallible independent comparator before writing
each phase receipt, and repeats the complete comparison before its top-level
success receipt. The expected input and 1,210 source publications come only from
the original-only ba5def0 capture in Actions run 39510538880, job 118930155954,
artifact 12305212845. The compressed observations and all original raw hashes
are recorded in `rust/crates/dev-tools/tests/fixtures/paramsd-daemon-original/README.md`.
Native output is never used to construct expected source values.

The complete parsed input remains exact, including packet bytes and service
order. The restart seed permits only its validated Event.logMonoTime word to
vary; every other serialized seed byte remains exact. Publication comparison
retains the original Event/debug numeric policy. GPS, persistence, all packet
journal entries, gate, signal, exit, cancellation and bounded shutdown remain
required. Corruption tests include input changes, a sub-tolerance one-bit seed
float change, framing, saved state, GPS and phase receipt changes. Fresh cloud
validation of this change is pending; the original Python comparator and
fixture helpers are still retained.
