# Rust model renderer component, issue 148

Issue: https://github.com/bin9208/openpilot-rust/issues/148

The isolated component ports both display ModelRenderer Widgets and shared path /
lane-marking geometry. Source/numeric contract:
[`DESIGN.md`](../../rust/crates/ui-application/src/onroad/model_renderer/DESIGN.md).
Portable checks and limitations:
[validation](../rust-port/ui-model-renderers-validation.md).

The worker's local `.omo/evidence/ui-model-renderers-148/INDEX.json` records exact
invocations, binary/source hashes and results. This private artifact index is not
a remote backup. Parent UI composition, integration review, whole-runtime startup
and log upload, target packaging, and user-performed device acceptance remain open.
