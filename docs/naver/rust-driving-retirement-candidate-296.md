# Driving tool retirement candidate (#296, #462)

This candidate adds an explicit `retired-driving` source policy for the four
historically pinned driving checker/reference Python files, including the shared
Desire helper. It requires their
absence and rejects reappearance, while continuing to hold and verify every
remaining source input. The default policy still requires the original files.
No Python file is deleted by this change.

The native fixture sealer copies opaque PKLs, all four compiled model bundles,
and independently captured original observations. It binds the complete file
roster, lengths, hashes, catalog/model relationships and capture identity. It
does not execute PKLs or replace the separate compiler-equivalence gate.
The retirement test runs the same six held executables against the checkout
and a private source tree with exactly those four Python files absent,
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
passed. E25 gate39635596188 executed all model/collector/fault traffic in both
roots, then failed only its final cross-root metadata assertion. Retained reports
show three source-call counts differ by one exact level10 camera-wait debug
message. The revised projection first accounts for every raw string record,
then removes only that precise debug multiplicity from the cross-root count.
Other compared fields and the original live source-call validation stay strict.
An authenticated25-file retained-data continuation avoids repeating completed
traffic solely for that wrapper fix. The new four-source closure still needs
its final hosted functional and eight source-reappearance controls.

The native archive stager supports a compact representation of the same complete
fixture. It regenerates only86 original synthetic NV12 images using the existing
NumPy-compatible PCG64 implementation, rejecting any whole-file SHA/size mismatch.
All original expected Cereal packets, model data and other observations remain
archived inputs. This removes311,042,048 bytes of redundant image storage without
changing a case or tolerance. The complete fixed fixture roster and final native
fixture verification remain mandatory; hosted regeneration proof is pending.
Archived CPU model objects retain their original target requirements. A generic
x86_64 label alone must not be assumed to establish cross-run CPU compatibility;
the driver reused-catalog warmup failure is being diagnosed separately.

No device boot, recording,
upload, CPU improvement or complete Python retirement is claimed. Device
acceptance remains user-owned.
