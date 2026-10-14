# GitHub runtime distribution (#274)

Issue: https://github.com/bin9208/openpilot-rust/issues/274

The user approved deployment work on 2026-10-13 after the source-only startup
gap in #269 was identified. The outcome is a normal source installation that
obtains its matching native runtime automatically. No vehicle is accessed.

## Distribution contract

- Use only `bin9208/openpilot-rust` releases for this runtime. Public assets need
  no device token. Tags are `runtime-<full source SHA>`; development candidates
  are prereleases. Never select an unrelated `latest` binary.
- A small static AArch64 installer and its SHA-256 sidecar bootstrap from the
  checked-out shell entry. The installer does not depend on the missing runtime,
  Python, a compiler, or an already initialized graphical application.
- Release `runtime-release.json` schema 1 contains `source_commit`,
  `architecture` (`aarch64`), `agnos_version` (`19.8-carrot-bt1`),
  `manifest_sha256`, and `archive` with `name`, `sha256`, `bytes`, and
  `unpacked_bytes`. Archive names are
  `openpilot-rust-<full source SHA>-aarch64.tar.gz`.
- The archive retains the sealed manifest layout from #269. Reject invalid
  paths, escaping links, duplicate members, unexpected files, wrong identity,
  wrong ELF architecture, and hashes/modes inconsistent with the manifest.
- Schema 1 permits at most 128 MiB in aggregate across the 17 allowlisted source
  assets, matching the installer's activation-space reserve. The whole archive
  remains bounded to 8 GiB unpacked and each release asset to less than 2 GiB.
- Stage complete verified files before activation. Preserve an installed
  generation on download/verification failure. Installation is serialized,
  interruption recovery is deterministic, and activation is completed before
  starting any runtime child. Space is checked before downloads/extraction;
  never delete user routes or settings to make room.
- Normal staged updater finalization must install the target checkout's
  runtime before setting `.overlay_consistent`. A package not yet published
  is a retryable update failure, never permission to run mismatched binaries.
- Keep existing AGNOS image URLs, model selection, log upload endpoints,
  scheduling, hardware policies and all validity checks. This changes runtime
  distribution only; it does not migrate every external service to GitHub.

## Validation and publication

Test fresh installation, idempotent reuse without network, missing releases,
wrong commit/hash, truncated downloads, interrupted activation, unsafe archive
entries, concurrent installers and low disk. Exercise the real CLI against
owned files/HTTP and use actual AArch64 installer under QEMU. Existing normal
bootstrap, overlay and updater checks remain required.

Promote the private deterministic archive procedure into tracked build-side
tooling with tests. Publication validates exact clean source, complete sealed
archive, static installer, required exact-SHA CI, and repository identity before
creating a draft. Publish only after all assets are present and verified; refuse
to overwrite an existing published tag. Download published assets and compare
hashes before reporting distribution complete.

## Implemented entry points and evidence

`launch_openpilot.sh` runs `rust/tools/ensure_runtime.sh` on AArch64 before
executing the native launcher. A valid installed package needs no network.
Interrupted installation journals or rollback directories force the verified
cached installer through recovery before the shortcut is allowed. The native
`--verify-package` command has no hardware/startup-child side effects and checks
Git HEAD as well as build metadata, preventing stale `build.json` from hiding a
source change.

The installer stores verified generations under `rust/.runtime-packages` and
switches `rust/.runtime-active`. Legacy directory migration and the 17 permitted
source assets have a durable recovery journal. The device requires 2 GiB free
plus calculated download/extraction growth; build-side tooling retains the
separate project-required 25 GiB reserve. Neither path deletes route logs.

The staged updater runs the target checkout's installer before making the
finalized source consistent. An absent installer in a Rust checkout or any
installation error leaves the update unavailable. Original non-Rust checkout
finalization retains its previous path.

Build-side commands, run from the repository root with a clean sealed checkout:

```sh
cargo run --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin package-runtime-release -- --root "$PWD" \
  --installer /path/to/static/openpilot-runtime-install \
  --installer-build-receipt /path/to/runtime-installer-build.json \
  --output /path/to/new-release-directory
cargo run --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin package-runtime-release -- --root "$PWD" \
  --output /path/to/new-release-directory --verify-only
cargo run --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin publish-runtime-release -- --root "$PWD" \
  --assets /path/to/new-release-directory --check-only
cargo run --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin publish-runtime-release -- --root "$PWD" \
  --assets /path/to/new-release-directory
```

The installer build receipt records schema 1, full `source_commit`, architecture
`aarch64`, target `aarch64-unknown-linux-musl`, `installer_sha256`, and the actual
`build_command` string. The publisher verifies the four separate dev push
workflows and all configured required checks for that exact source. It uploads
into a draft, downloads and hashes all six assets, publishes a prerelease, then
verifies downloaded assets again. This is verified local-build publication;
the generic CI cross-build is not substituted for the AGNOS runtime package.

Implementation evidence before publication:

- Real shell tests cover fresh bootstrap, offline reuse, missing release,
  corrupted installer and interrupted-install recovery routing (6 scenarios).
- Native launcher/updater tests and strict Clippy pass. Two tests run complete
  updater finalization with real owned Git checkouts and failing/successful
  installation commands. Six existing normal bootstrap scenarios also pass.
- Installer validation passes 29 CLI scenarios and three real curl/owned-HTTP
  scenarios, including a truncated response followed by a successful retry.
- Publication and existing sealing validation pass 66 focused tests, including
  the shared source-asset size boundary. The actual packaging CLI rejects
  129 MiB of source assets and accepts both the 128 MiB boundary and a 129 MiB
  runtime binary. A broader
  local tools run exposed missing test-provider environment; full configured
  workspace validation remains the exact-SHA CI gate, not that local run.
- A genuine preserved #269 package (`7c5935eb118d`) was installed with the real
  static AArch64 installer under QEMU, then checked again. Independent readback
  verified 1,074 artifact hashes, 17 source assets, 161 ELF headers and 30 links.
  This proves archive/install compatibility; it is not hardware execution or
  the final distribution revision's release receipt.
- Independent implementation review approved the change after reproducing and
  fixing the interrupted-install shortcut. Final publication evidence and
  exact commit/Actions/release URLs are recorded in issue #274.

The existing #269 archive remains preserved as the pre-distribution candidate.
Physical device execution, GPU numerical correctness, driving acceptance and
performance remain untested and user-owned under #1/#269.
