# Native Wi-Fi library

Issue [#135](https://github.com/bin9208/openpilot-rust/issues/135), used by shared UI [#125](https://github.com/bin9208/openpilot-rust/issues/125) under full-runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The first increment exposes typed `Snapshot`, `Command`, `Event`, network/security/metered/status types and `normalize_ssid`, with `WifiManager::{start,snapshot,drain_events,send,stop}`. Snapshot/events are delivered to the UI thread; the library owns its background D-Bus operations. It preserves connection epochs, strongest-network sorting, connection settings, scan cadence and source-disabled callback behavior. The standalone policy comparison executes unchanged WifiManager methods and matches 781 scenarios / 2,322 state snapshots exactly, including actions during delayed connection lookup.

The native method/signal implementation and examples compile and pass focused clippy. Actual private-D-Bus lifecycle validation, full source-method comparison and Actions/ARM acceptance remain in progress; this increment is not completed runtime evidence or a production selection change.

External dependencies are [dbus 0.9.12](https://docs.rs/dbus/0.9.12/dbus/), [dbus-tokio 0.7.6](https://docs.rs/dbus-tokio/0.7.6/dbus_tokio/) and the unchanged vendored libdbus boundary. Project-owned state, settings, event delivery and scheduling logic are Rust. Source main/monitor D-Bus connections are separate. Calls retain source indefinite reply waiting, with owned cancellation when stopping the manager. No Python runtime fallback, real system bus or host/vehicle networking operation is used by the host fixtures.
