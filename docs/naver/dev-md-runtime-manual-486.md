# Explicit exact-source runtime request — #486

The user requested publication after `git pull` and reboot left the updated
device unable to start. `launch_openpilot.sh` invokes `ensure_runtime.sh`, which
requires a source-matched installed manifest or downloads the release for the
actual Git HEAD. The unmarked CI merge has no corresponding release yet.
Device logs are unavailable; this records the verified source/release gap,
not a claim that other device faults have been excluded.

- Request: https://github.com/bin9208/openpilot-rust/issues/486
- Runtime source: `dc95138076836ea3a5c1b6c27262be3f3b912f71`
- Required push CI: https://github.com/bin9208/openpilot-rust/actions/runs/39699486761
- Existing successful PR CI: https://github.com/bin9208/openpilot-rust/actions/runs/39603474222
- Controller branch: `codex/fix-486-exact-runtime-delivery`

The original automatic marker-only workflow remains unchanged on dev-md. This
issue branch adds a manual dispatch input referencing an open owner-authored
issue containing the exact source SHA, push CI run, controller ref and explicit
publication authorization. Owner identity, actual merged PR, current dev-md
head, completed latest push CI and all five successful exact-source gates are
checked before any package build. An incomplete CI run is waited for, never
accepted as success. Pure approval/evidence tests run in the normal fast CI.

The immutable requested source is checked out first, preserving its existing
temporary storage and Rust PATH initialization. The controller is retrieved at
the exact dispatch commit into owned runner temporary storage. The requested source's
`package.py` is loaded as a library, without modifying its marker-only entry
point or any runtime source. Its own source policy, baseline/sysroot/provider
verification, complete AGNOS build, sealing and offline installer checks are
used. Publication verifies the exact-source publisher and installer hashes;
that native publisher independently verifies latest exact-SHA CI and all
sealed assets before publication. The source's online installer verification
then checks the public release. Controller approval receipts accompany the
Actions evidence artifacts, separate from the six runtime release assets.

Local validation is limited to syntax, formatting and diff review. Functional
tests and all builds/install verification run in GitHub-hosted Actions. The
runtime source and protected branch are not changed by the controller; device
connection, installation and reboot remain user-owned. Publication and online
verification are pending; append exact Actions/release evidence to #486.

## 2026-10-22 sealed package / publication handoff

Run [39771670638](https://github.com/bin9208/openpilot-rust/actions/runs/39771670638)
completed the genuine exact-source AGNOS build, ABI audit, sealing and offline
installation. The archived `offline-install.json` records source
`dc95138076836ea3a5c1b6c27262be3f3b912f71`, installer exit 0 and installed
AGNOS launcher verification exit 0 (`device_execution: false`). The sealed
archive hash is `afd4775ecc24c2d9b8e68339d5b1def60c4d06b44a63bcaa0ff16242b9f9fea4`.

Publication job 119828066569 correctly rejected `archive mode/identity mismatch`
before publishing. The immutable source's restore function extracts with Python
`filter="data"`, which removes group/other write bits and adds owner read/write
bits. The native publisher compares the archive modes against the restored
filesystem's exact modes. The manual controller now restores ordinary permission
bits from the hash-verified sealed archive after safe extraction, logging each
actual difference. It rejects escaping paths, symlink substitutions, unsupported
members and special mode bits. Archive bytes, source files, native publisher and
all independent archive/source/CI checks remain unchanged. A cloud regression
covers 0664/0775/0444 round trips and rejects an altered archive digest.
Public release and online installation remain pending; no device boot claim.

## Completed exact-source delivery (2026-10-22)

- Source: `dc95138076836ea3a5c1b6c27262be3f3b912f71`; pinned external
  controller: `32237791fb0b23d702c064da36da658fcb650cfd`.
- [Delivery run 39783809670](https://github.com/bin9208/openpilot-rust/actions/runs/39783809670)
  succeeded: approval/CI job 119841603917, genuine AGNOS build/seal/offline job
  119841707115, native publication/online install job 119873057051.
- [Public exact-source prerelease](https://github.com/bin9208/openpilot-rust/releases/tag/runtime-dc95138076836ea3a5c1b6c27262be3f3b912f71)
  is not a draft and targets the exact requested source. All six assets exist:
  the ARM64 installer and checksum, exact-source runtime archive and checksum,
  `runtime-installer-build.json`, and `runtime-release.json`.
- Public archive SHA-256:
  `3306896fb204e076daae8164d37665119343a3f1058857315c1446fd9f83e9a1`.
  Runtime manifest SHA-256:
  `827cf40067fa50745538ad28c5abdfaef9fbf643090e89d088cca095e0a53395`.
- Offline and public online evidence agree on that source/archive/manifest.
  Both actual ARM64 installers exited 0 and both installed AGNOS launchers
  passed `--verify-package` under QEMU with exit 0. `device_execution` is false.
- Fresh OpenCL receipt records the requested source, `fresh_build: true`,
  `opencl: true`, and `source_unchanged: true`. The original native publisher
  independently passed exact-SHA CI, sealed assets and publication checks.
- Successful logs confirm the prior mode failure: 17 OpenCV license/provider
  companion files needed `0644 -> 0664` restoration after safe extraction.
  The sealed bytes and original verifier remained unchanged.
- Build evidence artifact 12431213884 and publication evidence artifact
  12433341119 retain the receipts. Local concise evidence is indexed under
  `.analysis/archive/2026-10-22-cicd-runtime-486/` (ignored, not a remote backup).

This completes the authorized publication request. CI/CD #409 was already
accepted independently. No further dev-md integration/rebase, dev/main merge,
device connection, installation or reboot was performed. Normal physical-device
boot and driving acceptance remain user-owned and have not been established.
