# Issue #115: native TICI eSIM LPA

Tracking: [#115](https://github.com/bin9208/openpilot-rust/issues/115), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The isolated native LPA library and JSON operation entrypoint replace the project-owned serial/APDU and ES9+ profile-management algorithms. Scope, unchanged-source oracle, actual owned PTY/TLS lifecycle, dependencies and limitations are recorded in [the validation document](../rust-port/lpa-validation.md). The inventory identifies this as host-validated protocol/adapter work with production selection unchanged.

Nine focused scenarios passed locally: six exact source/native comparisons, native binary flock/channel lifecycle, TLS hostname/root rejection, and the HTTPS read deadline. No physical modem, SIM, carrier, vehicle connection or provisioning occurred. Integration owns exact-SHA Actions, generic ARM compilation and eventual runtime adoption. This slice does not satisfy the full-runtime or first-device-test gate by itself.

Docs-Not-Needed: internal runtime replacement with no user setting or guide workflow change.
