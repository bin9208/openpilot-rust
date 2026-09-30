# Rust statistics runtime tracking

Issue: [bin9208/openpilot-rust#75](https://github.com/bin9208/openpilot-rust/issues/75).
The implementation and source-oracle evidence are documented in
[statsd-validation.md](../rust-port/statsd-validation.md).

This is an isolated native producer/continuous-daemon increment of full-runtime
issue #1. Production selection, vehicle acceptance and measured performance remain
separate. The integrating task records exact-SHA Actions and PR results; no
original-fork synchronization, vehicle connection or NAS deployment is authorized.
