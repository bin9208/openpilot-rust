# GNSS integration record (#137)

[Issue #137](https://github.com/bin9208/openpilot-rust/issues/137) combines the preserved, source-compared native u-blox and Qualcomm candidates. The scope, required CI checks and remaining device limits are recorded in [GNSS runtime integration](../rust-port/gnss-runtime-integration.md).

Local workflow-policy validation passes all 15 tests, including the required GNSS job and failure propagation through the aggregate gate. Exact-SHA Actions results are tracked on the issue and integration pull request. The full runtime issue remains open until normal startup and existing log upload are composed; these component checks are not a request for a C3X test.

Docs-Not-Needed: isolated experimental GNSS composition and CI policy with unchanged production behavior.
