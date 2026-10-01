# Issue #119: native AGNOS and image-casync

Tracking: [#119](https://github.com/bin9208/openpilot-rust/issues/119), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

The native library and CLI preserve AGNOS cache/hash/sparse/casync/slot/retry behavior using existing native HTTP and a pinned static liblzma binding. [Validation and integration seams](../rust-port/agnos-validation.md) record the unchanged-source comparisons and the actual CLI proof.

Ten focused source/native scenarios passed locally, including a sixth transient download attempt, bounded certificate/permanent/invalid-URL failures, hash and truncated-response rejection, seed/target/remote SHA-512/256 reconstruction, and real owned-file CLI lock/flash/verify/swap behavior. Regular-file `wb+` truncation is deliberately preserved and separately described; these files are not claimed to emulate block-device semantics.

No device, real boot slot or production startup selection was changed. Parent integration owns updated #118/startup UI wiring, exact-SHA Actions and generic ARM validation. Directory/tar casync remains outside the AGNOS image-only port. Whole-runtime startup/upload and device acceptance remain pending.

Docs-Not-Needed: internal native updater implementation, preserving existing automatic-update behavior and settings.
