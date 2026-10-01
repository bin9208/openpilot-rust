# Microphone runtime port

[#126](https://github.com/bin9208/openpilot-rust/issues/126) implements native
microphone capture, raw audio and sound-pressure publication under full runtime
#1. [Validation and dependency boundaries](../rust-port/micd-validation.md)
describe the actual original-source comparisons and owned native ABI/IPC runs.
The process catalog exposes the candidate without selecting it in production.
The first user device comparison still requires all project-owned runtime
components, normal startup and the existing log upload path.

Docs-Not-Needed: internal candidate with source behavior retained; no selected
runtime or user setting change.
