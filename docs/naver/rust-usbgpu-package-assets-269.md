# Native eGPU package assets (#269)

The installed `rust/bin/usbgpu-assets` now contains 45 genuine files: the
previously verified 43-file native export, the 776,634,338-byte source-pinned
Cinque v3 PKL, and a typed `native-provider.json`. Three license/provenance files
are installed under `rust/bin/licenses/usbgpu`. The complete staged path, size
and SHA-256 list is `.omo/evidence/269-usbgpu-assets/staged-files-final.json`.
The manifest SHA is
`1bb0c80c1f75dc757ad104162263eb87c0107385fab9ea563a7f9f98b46bf91e`;
the model SHA is
`e758b96df27858ea97122d18554930d04f9f8bda417417074edfb3a72b008d0b`.

`rust/tools/stage_runtime_usbgpu.py` checks the retained manifest, every declared
companion, both QCOM warp packages, the source-pinned firmware hashes and the
original `CINQUE_V3_MODEL` PKL identity before copying. Existing conflicting
files and directory symlinks fail admission. Repeated staging is idempotent.
The genuine checkpoint is
`b9facbcc-4d47-410e-b3ce-dfcbad12ba92/56320/f78ed37d-afad-4dbc-8050-40ea885eedde/12864`.
It supplies 18,452 model outputs and both 1344×760 and 1928×1208 native camera
companions. The retained runtime archive SHA
`a25b81e90d5259c27bc49b3a93ab499c1909a33e133cba974cf023cd94727eeb`
is provenance only: the archive and its Python modules are not installed or
executed. The original export provenance also records the checksum of an owned
catalog used during the earlier build comparison. That catalog used an owned
test endpoint; it is not a trusted production remote catalog and is not shipped.
No runtime URL is invented by the native producer record.

The model delivery consumer accepts this provider only for the exact original
source-pinned generic SHA, size and precompiled source family. It copies verified
PKL bytes into the writable normal source cache and native precompiled cache;
package files are never hardlinked into those writable caches. The original
state activation, transient failure handling, persistent rejection and device
validation-key paths remain connected. An explicit different source selection
does not use the pinned provider. An absent provider retains the existing remote
delivery behavior. Readiness verifies the cached PKL digest and caches only an
unchanged path/device/inode/size/mtime/ctime fingerprint with nanosecond times;
the worker independently verifies the digest before opening a GPU.

Actual staged ARM release binaries passed QEMU artifact admission at both camera
resolutions using their default sibling asset root. With an empty owned device
directory and owned `UsbGpuHardwareSeen` Params, the normal model CLI sequence
`--ensure-if-egpu`, `--boot`, `--ready`, `--active-sha` activated the original
model in a fresh owned cache and reported `prepared=true`. All four commands
made zero network `connect` calls; no remote `installed.json` or runtime archive
was created. Exact commands, environment and output are recorded in
`normal-startup-final-commands.json` and `normal-final-*.log` under the evidence
directory. The first sequence had a harness-only JSON parsing error because
boot emits a diagnostic before its final JSON line; the final fresh sequence
parses that line and completes all four consumers.

Real consumer controls reject a same-size cached PKL mutation, a changed asset
manifest and a different selected model. Restoration returns readiness. The
same-size mutation originally passed readiness; that RED is preserved in
`same-size-readiness-red.log`. The hot-process regression then kept device,
inode, size and mtime unchanged while changing ctime: readiness became false,
worker preparation failed, and restoration became ready. The actual syscall
trace contains one initial PKL read and no extra PKL reads across nine unchanged
readiness polls. `consumer-controls-final.json` and
`hot-admission-trace-result.json` retain the controls and observables. One early
control had an inaccurate scenario name; its recorded arguments identify the
positive default-root 1928×1208 check, while the separate altered-root worker
case proves rejection.

All-target strict Clippy, the two provider regressions plus two model tests,
host examples, and ARM release binaries passed. These staged executables were
built from dirty integration source and are pre-commit QA artifacts. The final
sealed candidate must rebuild the full release after its source commit.

This is offline setup and structural admission, not trained-model GPU numerical
or device acceptance. The historical `comma-run-model` family still lacks its
matching `model_runtime.py` queue artifact; no contract was guessed. Local
`run_policy` code and small source graphs are engineering evidence, but a genuine
trained local-family PKL/export is unavailable and those fixtures are excluded
from the installed package. Full startup/upload acceptance, final committed
package identity and physical GPU comparison remain separate gates.
