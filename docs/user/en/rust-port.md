# Experimental Rust port

This independent repository provides a project-owned Rust runtime candidate and
a source-matched AGNOS installation path. Remaining source conversion and device
and vehicle acceptance are tracked separately in #1/#296. Install only a verified
AGNOS release matching the exact checkout, rather than a generic host/ARM build.

On a Linux development host, with Rust 1.94.0 installed, run from `rust/`:

```sh
cargo run --release --locked --bin cpu-sample -- 1000
```

The optional interval is 1–60000 milliseconds (default 1000). Output is TSV.
100% means one logical core; values above 100% are valid for multi-threaded
processes. The processor column is the last observed main-thread core, not the
process's affinity or the placement of every thread. Unreadable/exited processes
and invalid counter deltas are omitted. Short intervals have coarse resolution.

This is a one-shot diagnostic, not a background service. It does not publish
vehicle commands. Host tests and generic aarch64 builds do not validate AGNOS
compatibility, CPU savings, temperature improvements, or safe vehicle operation.
Do not deploy the generic CI build to a vehicle; target validation remains open.

## Follow-up candidate for older defects

The 2026-10-23 #497 working branch is validating the following corrections. They
are not all present in the existing public release; installation and driving
acceptance remain separate.

- Oversized torque DEBUG point lists are omitted with a diagnostic.
- Failed completion markers for skipped upload files are reported and retried.
- Raw-video write, flush and close failures retain the incomplete marker and report the error.
- Encoders reconnect after a camera server replacement. A changed buffer layout requires a safe restart.
- External Jetlink frames have no raw prediction tensor. With `SEND_RAW_PRED`,
  normal external publication continues with an empty raw field. Native-model raw output remains available.
- Numbered driver warnings appear in Dashcam reports, and live navigation displays use the actual instruction fields.
- Hyundai CANFD alternate buttons, Tesla cancellation events and Volkswagen MQB initial state have separate correction gates. Missing vehicle torque data or DBCs receive no invented replacement.
- Terminal reset and shutdown also clean up jobs owned by that session.

These corrections do not establish CPU savings or normal operation on every
vehicle. The lateral smoothing correction uses model uncertainty while retaining
the existing interpolation range and cap; separate vehicle acceptance is required.
