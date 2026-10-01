# Native bridge issue record

[#121](https://github.com/bin9208/openpilot-rust/issues/121) replaces project-owned
C++ bridge orchestration with Rust, preserving original ports, service selection,
non-conflated packets and connection-driven queues.

[Validation and native dependencies](../rust-port/bridge-validation.md) record
the source/native real IPC/TCP comparison and remaining integration limits.
Host packets match in both directions; queue activation retains other backlogs.
Full-runtime #1 and user device acceptance remain open.

Docs-Not-Needed: internal candidate with unchanged selected runtime.
