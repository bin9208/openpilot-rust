# Native USB companion staging (#296)

`stage-runtime-usbgpu` replaces `rust/tools/stage_runtime_usbgpu.py` with native
Rust in `openpilot-dev-tools`. It accepts the same named root, package,
expected-manifest, firmware-license and optional model arguments. The optional
model uses the runtime's original `pinned_manifest()` identity/size instead of
parsing `big_model.py`; the tool needs no Python interpreter or Python source.
The dependency disables USB runtime default features.

Admission retains the original selected manifest hash, version/file-count/model
identity checks, individual size/hash/duplicate checks, required licenses and
provenance, eight source-pinned AMD firmware hashes, both camera warp graphs and
every referenced kernel, and the 128 MiB companion limit. Optional offline model
admission requires the exact original source-pinned PKL, its descriptor size and
checkpoint, and produces the same ordered provider JSON. No remote catalog or
runtime URL is synthesized. Destination conflicts/symlinks fail admission;
repeated identical staging retains existing files. The shared disk guard checks
25 GiB plus growth, including the generated provider record.

The native implementation also bounds copy reads and freezes/rechecks every
source before publishing. Temporary frozen files are linked into their final
same-filesystem paths and the temporary directory is removed; outputs never
share an inode with the mutable input provider. Reserved manifest/provider paths
cannot be declared as companions. Existing files are rechecked before reuse.
This does not add a transactional guarantee against concurrent directory edits.

## Evidence, 2026-10-14

- RED: four of seven new integration tests failed against the explicit
  not-implemented stub, including successful staging and empty-PATH CLI cases.
- GREEN: seven integration tests cover staged bytes/licenses/idempotence;
  manifest identity/version/model/size/hash/duplicate/count controls; missing
  required files and camera kernels; changed firmware pins; path traversal,
  source/destination symlinks, conflicts, oversized companions and wrong optional
  model; the CLI runs with an empty PATH. A unit regression verifies copy growth
  cannot exceed the reserved bound by more than its one-byte rejection probe.
- Full `cargo test --manifest-path rust/Cargo.toml --locked -p
  openpilot-dev-tools` passed 118 tests. The focused seven tests passed again
  after strengthening the maximum-file-count assertion. Strict all-targets
  Clippy, scoped Rustfmt and diff checks passed. Each build used the coordinated
  cache, two jobs, disabled incremental compilation and a disk reserve check.
- An owned-root rehearsal used the retained genuine #269 companions with manifest
  `1bb0c80c1f75dc757ad104162263eb87c0107385fab9ea563a7f9f98b46bf91e`
  and exact 776,634,338-byte model
  `e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b`.
  The native tool ran with an empty PATH and no Python source in its root. Its
  complete `rust/bin` tree was byte-identical to the original Python tool's
  result: 48 files and 780,342,357 bytes, including all licenses and the exact
  `native-provider.json` bytes. The original script was deleted only afterward.
- Changing one byte of an owned copy of that full model without changing its
  size made the native CLI exit 1 with the exact source-pinned-PKL error. The
  genuine retained input was never modified. Free space remained above 379 GiB.

No dedicated tests or executable callers of the former staging script were
present. `test_usbgpu_catalog_package.py` tests a different, still-unconverted
exporter and remains intact. This stager does not execute a GPU/model, contact a
server/device, establish numerical inference equivalence, or complete the wider
Python development-tool retirement. The full source-matched target rebuild and
normal startup/recording/upload remain separate parent-task gates.

Docs-Not-Needed: internal build-tool replacement with equivalent artifact
admission; no user setting or user-facing selection policy change.

## Review correction: destination parent types

Independent review found that symlink-only ancestor checks missed an existing
regular file at `rust/bin/licenses` or `rust/bin/licenses/usbgpu`. Child existence
checks return false below such a file; publication then installed the companions
before failing to create the license directory. The new regression reproduced
that partial publication before the fix. Ancestor preflight now requires every
existing ancestor to be a directory before freezing or publishing any files.
The focused eight-test suite passes both parent-file cases, checks that no
`usbgpu-assets` directory was created and retains the unrelated file's exact
bytes. Concurrent directory mutation remains outside the staging guarantee.

After this correction the complete dev-tools suite passed all 124 tests
(including the independently added five torque checks and eight USB staging
checks). Strict all-target Clippy, scoped Rustfmt and diff checks passed again.
The final full-suite log is retained locally as
`.analysis/archive/2026-10-14-usbgpu-stage-296/review-fix-tests.log`.
