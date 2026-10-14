# Native torque numerical provider staging — issue #296

On 2026-10-14, `openpilot-dev-tools` replaced
`rust/tools/stage_torque_numerics.py` with the native `stage-torque-numerics`
binary. The implementation is `src/torque_stage.rs`; six CI callers and the
torqued third-party notice now refer to the Rust tool. No existing dedicated
Python staging test file was present. The Python numerical reference/oracle
families remain separate, and this change does not alter numerical runtime
algorithms, sampling, tolerances, or provider ABI.

The tool requires exactly one NumPy 2.5.3 package and matching wheel filename,
SHA256, and declared archive size from the selected `uv.lock`. The lock defaults
to the current directory's ancestor repository; `--lock PATH` supports explicit
source selection. The archive is bounded and frozen in owned memory before ZIP
inspection. Only `numpy.libs` members and original distribution license files
are staged; Python modules are excluded. All dependent native libraries retain
their original names and hashes. The manifest remains format 1 with ABI
`scipy_dgesdd_64_`, exactly one `libscipy_openblas64_*` library, original archive
ordering, and original provenance metadata.

The original directory-entry compatibility is preserved: `numpy.libs/` produces
an empty regular file named `numpy.libs` and a corresponding empty-file manifest
hash. This is part of the original metadata contract observed from the real
source-locked wheel. It is not a dynamically loaded library.

Staging rejects traversal, backslashes, symlink/special ZIP members, duplicate
names, flattened output collisions, changed locks/archives, absent/ambiguous
OpenBLAS, and existing output paths. Selected expansion is bounded to 512 MiB,
each member/archive to 128 MiB, and staging checks the 25 GiB disk reserve plus
estimated growth. An owned temporary directory is published with atomic
no-replace rename only after successful extraction and metadata creation.

Validation:

- RED: the new contract test failed against an unimplemented native stage.
- GREEN: five native tests cover manifest/dependency hashes, nested licenses,
  Python exclusion, existing output, lock version/hash/filename/size/duplicate
  admission, path traversal/collisions, missing/multiple OpenBLAS, ZIP/output
  symlinks, and CLI operation with an empty `PATH` outside the source directory.
- Strict all-target Clippy passed for `openpilot-dev-tools`.
- The coherent full `openpilot-dev-tools` suite passed all 123 tests.
- The actual locked x86_64 wheel
  `numpy-2.5.3-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl`
  (`b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3`)
  was staged by both the original Python tool and the native binary before
  Python retirement. All 23 files compared byte-identically, including original
  license trees, numerical providers, manifest, provenance, and the empty marker.
  Manifest SHA256 is
  `2a5b2cf851b28400d286c06de0beb7c3ab9a21fd5c3e8b1ae860a9fa33f3efbf`;
  provenance SHA256 is
  `50e8063e5d155c651887877c1113881339bf9c15e34650ad8ea2bcf717b7eaff`.
- The CI policy now requires the native staging command in the radar job;
  all 24 policy tests and 367 subtests pass without weakening other checks.

Ignored local reproduction/oracle records reside under
`.analysis/scratch/2026-10-14-native-torque/`. NumPy-distributed OpenBLAS/Fortran
providers remain pinned external native dependencies with their original
licenses. This validates staging equivalence, not fresh numerical execution or
device/drive acceptance.
