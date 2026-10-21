# Driving tool retirement candidate (#296, #462)

This candidate adds an explicit `retired-driving` source policy for the three
historically pinned driving checker/reference Python files. It requires their
absence and rejects reappearance, while continuing to hold and verify every
remaining source input. The default policy still requires the original files.
No Python file is deleted by this change.

The native fixture sealer copies opaque PKLs, all four compiled model bundles,
and independently captured original observations. It binds the complete file
roster, lengths, hashes, catalog/model relationships and capture identity. It
does not execute PKLs or replace the separate compiler-equivalence gate.
The retirement test runs the same six held executables against the checkout
and a private source tree with exactly those three Python files absent,
including all six model cases, six collector cases and logging fault checks.
The private tree uses a disclosed read-only Git metadata pointer for the
existing source-commit check; it does not restore the retired source files.

Issue [#462](https://github.com/bin9208/openpilot-rust/issues/462) records a
late-finalization bug: a failed last verification could leave an owned PASS
report. The new guard removes that report unless verification and publication
both succeed. Verification runs before and after stdout write/flush. A foreign
replacement report is preserved. Already emitted stdout cannot be retracted;
acceptance requires exit zero and the retained disk report together.

The proposed hosted gate includes the actual old `Lines::until` method for
the #461 EINTR negative and a separately labelled controlled no-rollback
negative for #462. Fixed-source unit, Clippy, original comparison and complete
private-absence checks must pass afterward. Source and formatting review have
passed; these new candidate gates have not yet run. No device boot, recording,
upload, CPU improvement or complete Python retirement is claimed. Device
acceptance remains user-owned.
