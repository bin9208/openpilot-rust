# Driver monitoring policy and packet validation

Issue [#29](https://github.com/bin9208/openpilot-rust/issues/29), part of
[full runtime #1](https://github.com/bin9208/openpilot-rust/issues/1).
Implementation base: `6ed3b4bc32fb538faf6f00ff386f55c4ee6cbe34`.
Source: `openpilot/selfdrive/monitoring/policy.py`, `common/stat_live.py`,
`common/filter_simple.py`, `common/realtime.py`, and the original cereal schema.
The oracle records each source SHA-256 in `report.json`.

## Scope and interface

`openpilot-monitoring` implements the production default policy, running scalar
statistics, and complete `driverMonitoringState` Event encoding. Its input has
fixed model vector dimensions, explicit missing arrays, original car/calibration
scalars, and demo mode. `DriverMonitoring::new` accepts the saved wheel side,
always-on mode, and the original `DriverTooDistracted` Params value. No parameter
names, source settings, thresholds, production selectors, or user guides change.

The policy runs at the original 0.05-second cadence. It retains vision alert
5/8/13-second and wheeltouch 5/15/25-second thresholds, the 5-second no-response
window, two-red/one-no-response lockout, 36,001-frame lockout expiry, independent
awareness histories, recovery factors, low-speed and always-on exemptions, and
the orange-alert guard against camera-hiding. Pose calibration retains raw
priors, variance-gated admission, 1,200 accepted samples, and 7,200-sample capped
weighting. Wheel-side calibration and engaged-side freezing remain unchanged.
Sleep, phone, blink, confidence, steering allowance, and calibrated offset limits
follow the source operations and order.

All packet fields use the unmodified cereal bindings, including validity,
caller-supplied monotonic timestamp, alert and lockout counters/percentages,
force-deceleration, both awareness timers, all calibration fields, pose,
uncertainty, distraction types, RHD, and interaction. Internal arithmetic is
Float64; Float32 conversion occurs only at the original packet boundary.
NaN percentage conversion and Int8 counter overflow return an error instead of
silently saturating. Python min/max first-argument NaN behavior is preserved so
an uncertain model cannot accidentally become confident. JSON rejects non-finite
numbers and malformed fixed vectors; absent arrays preserve the original early
return. The original policy's optional custom test settings are not a runtime
configuration interface in this crate.

## Reproducible host checks

Run from the repository root with Rust 1.94.0, capnproto, numpy 2.4.6 and pycapnp
2.1.0. Python is used solely as an original-source test oracle.

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-monitoring --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml -p openpilot-monitoring --locked
PYTHONPATH=.:tinygrad_repo:rust/tools python rust/tools/check_monitoring_reference.py \
  --binary rust/target/debug/monitoring-probe --output /tmp/monitoring-policy
```

The oracle executes the actual original class/function ASTs; it does not
reimplement the reference policy. It substitutes only Params initialization and
message allocation adapters for unavailable host services. The original camera
size, focal length, timestep, and all policy settings come from source. Each
original `run_step` and `get_state_packet` is executed. The full original scalar
state is compared, excluding settings, redundant previous mean/variance copies,
and the constant no-response timeout. Both complete binary Event streams are
decoded through original pycapnp and every field is compared. Serialization
layout bytes may differ; decoded values must not.

Predefined acceptance: exact discrete states/counters; maximum absolute scalar
error `1e-10`; exact decoded Float32 packet values. JSON uses `float_roundtrip`
to preserve the original inputs at branch thresholds.

2026-09-30 host result: **209,144 frames and 209,144 packets passed**, maximum
scalar error **0**. Thirty-two histories cover both RHD defaults: attentive
calibration beyond the capped-count boundary, sleep/red/no-response/lockout and
full recovery, repeated red, missing face, uncertainty fallback/reset,
orange/hide/recovery, wheel-touch recovery, stopping/launching, always-on/gear,
demo overrides, engaged wheel-side switching, missing arrays, scalar thresholds,
saved lockout expiry, calibrated offset limits, and 24,000 seeded random inputs.
Coverage includes all four alert levels, both policies, calibrated/uncalibrated
pose, and locked/unlocked state. The unchanged source monitoring suite also
passed all 20 tests using the same host adapters.

Nine Rust regressions cover terminal sleep alert recovery in both RHD modes,
orange camera-hiding protection, exact uncertainty fallback frame, exact saved
lockout expiry, low-speed/always-on limits, invalid packet scalars/counters,
NaN confidence, malformed JSON/vector inputs, and CLI usage failure. Before
implementation the new crate test invocation failed because the crate did not
exist. The additional NaN-confidence regression failed before the Python-style
min/max correction. Final fmt, clippy, focused tests and Python lint passed.

## Evidence and remaining work

Local capture directory: `.omo/evidence/monitoring/` in the issue worktree.
Durable local archive: `.analysis/archive/2026-09-30-rust-monitoring/issue29/`
in the parent repository. `index.json` records exact invocations, binary
observables, artifact hashes and implementation SHA. Raw synthetic JSON and
binary streams are compressed in that archive, outside Git. CI repeats the
source/packet oracle in `rust checks`; exact-head Actions and independent review
are integration gates owned by the parent task, not claimed by these local runs.

The continuous `dmonitoringd` message loop, cadence/subscription validity,
parameter refresh/persistence and manager integration remain a follow-up.
`dmonitoringd` stays **not_ported** in the process inventory; only this policy
library is marked host validated. No vehicle was accessed or tested. No AGNOS,
full startup/log-upload candidate, device behavior, or CPU savings are claimed.
The full runtime must be completed before the user's first device comparison.

Docs-Not-Needed: this adds an isolated Rust policy library and host parity checks;
production settings and user-visible runtime behavior are unchanged.
