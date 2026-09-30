# Native managed child entry boundary (#86)

- Issue: https://github.com/bin9208/openpilot-rust/issues/86
- API contract, source provenance, actual child/collector/SDK comparisons and
  limitations: [managed-entry validation](../rust-port/managed-entry-validation.md).
- This is the reusable in-process prerequisite. Existing daemon entrypoint adoption
  and complete manager initialization remain separate integration work.
- No production selection, user settings, original Python, device or deployment
  change is included. Full runtime issue #1 remains open.
