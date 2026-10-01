# Native platform runtime integration

Issue: [#116](https://github.com/bin9208/openpilot-rust/issues/116), under the complete runtime gate [#1](https://github.com/bin9208/openpilot-rust/issues/1).

Manager initialization and continuous process ownership, serial modem supervision and hardwared state publication are combined with their original source comparisons and owned host execution fixtures. The mandatory `rust platform runtime` job participates in `rust checks` alongside the inherited integration and documentation gates.

See [platform runtime validation](../rust-port/platform-runtime-integration.md) for scope and component evidence. Prior integration head 0f575e5c passed Rust Actions run 36756279438 and Fast checks 36756279390. The branch then merges dev after hardware integration #122, and requires fresh exact-head push/PR/ARM results. Component host checks do not establish AGNOS or device acceptance. Complete runtime startup and log upload remain open under #1.
