# Native manager catalog and predicates (#96)

Tracking: [issue #96](https://github.com/bin9208/openpilot-rust/issues/96),
full runtime [issue #1](https://github.com/bin9208/openpilot-rust/issues/1).
Source: `openpilot/system/manager/process_config.py` and its unchanged
`process.py` constructors, inherited with their existing licensing/provenance.

`rust/crates/manager-catalog` carries all 63 registered descriptors in source
order, including disabled entries. It preserves native argv/cwd, Python module
provenance, the persistent daemon's PID key, enabled flags, kill policy and
restart policy. The commented-out uploader remains absent. Fifteen entries
identify an existing isolated Rust candidate; the other 48 remain explicitly
unported. Candidate metadata refers to `rust/port-status.json` for component
limits and never authorizes manager selection or Python fallback.

`ImportConfig` separates once-per-import flags from per-cycle predicates.
`from_environment(bodyteleop_available)` snapshots `/TICI`, host OS, environment
presence of `USE_WEBCAM` (even empty or non-UTF-8), and the exact `"1"` value of
`CARROT_WEB_EXTERNAL`. The embedding installation supplies bodyteleop module
availability without launching a Python import. Explicit configs permit the
same controlled PC/TICI/OS/import combinations as source testing. The caller
owns enabled/blocklist filtering and process supervision; evaluating a callback
alone intentionally does not apply enabled flags.

`Predicate::evaluate` retains ordered `All`/`Any` short-circuits and original
Params/GPS access ordering. In particular, offroad ublox still checks GPS paths
and persists changed `UbloxAvailable`, while offroad qcomgps does not probe GPS.
Logging can read `DisableLogging` even offroad for notCar. The wide YouTube
predicate reads the typed `UseWideCamera` default before live/quality values.
Cluster/YouTube ordinary parameter exceptions are caught at their original
boundaries; malformed or overflowing integer Params are fatal typed errors.
The unchanged Cython `get_int` aborts the reference process on those values even
inside `except Exception`. Native typed-error propagation is **not** a claim of
literal SIGABRT/process-boundary equivalence; manager adoption must preserve a
fatal result rather than silently treating it as false/default.

The native Params adapter uses the existing beepd signed 32-bit `std::stoi`
conversion. It preserves prefix parsing, whitespace and overflow rules, exact
raw boolean `1`, absent/empty defaults, and the C++ getter's empty return on
filesystem read failure. It also preserves Cython ignoring failed `putBool`
return codes. Unknown-key errors remain distinct, though catalog keys are fixed.

## Validation

`rust/tools/check_manager_catalog.py` invokes the unchanged original manager
constructors and callbacks through the real original Cython Params binary,
then compares the native example's JSON results. The source fixture substitutes
hardware/import availability and GPS existence, and stubs the unused Sentry
import only; it never launches/prepares children. Params data is written as raw
fixture files to cover bytes rejected by the Python typed setter. All callback
reads/writes still use the real binding and real on-disk native Params.

The complete comparison passed on the host and generic GNU aarch64 under
QEMU, using `catalog_probe`: 10,573 predicate cases and 24 fatal integer cases
per architecture. It covers all 64 import-flag combinations, 25
actual-environment snapshots (including empty and invalid-UTF-8 values), full
catalog callback states, boolean/integer/default semantics, ordinary exceptions,
directory read/write failures, GPS side effects, and access-order short circuits.
Fatal malformed integers run in independent original processes with core dumps
disabled; original SIGABRT is compared to native typed fatal errors and identical
access prefixes. Candidate names/package/bin paths are checked against the
inventory and Cargo manifests.

Evidence and exact invocations are retained in
`.omo/evidence/manager-catalog-96/final/evidence.json` with executable/source
hashes, nonempty per-scenario artifacts and final committed SHA. The parent
integration owns cloud checks and manager wiring. Normal startup, manager
initialization, process launch, complete upload integration, AGNOS behavior,
vehicle acceptance and measured CPU savings remain pending. No device was
connected and no production daemon selection was changed.
