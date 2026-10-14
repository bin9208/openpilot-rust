# Native Panda runtime firmware tools (#321)

The native `build-runtime-panda` replaces `rust/tools/build_runtime_panda.py`.
It builds and verifies the existing F4/H7 main firmware and bootstubs directly
with Arm GNU Toolchain 13.2.rel1 (13.2.1 20231009). It never flashes or connects
to a device. Work began at `7c8b2f0f05b1140d6d391275d3643ba3daa769b1`; the parent
advanced this branch to `430ec72bf4fd5bc53dafb0e262b815e76c38ff37` without changing
Panda inputs. These are host builder/parity results, not vehicle acceptance.

Example invocation after building the native developer tool:

```sh
build-runtime-panda --root /path/to/checkout --output /path/to/new-owned-build \
  --toolchain /path/to/arm-gnu-toolchain --jobs 2
build-runtime-panda --root /path/to/checkout --output /path/to/new-owned-build --stage
```

`--compiler` remains an alias for `--toolchain`. Jobs must be one or two; serial
compiler execution stays below that ceiling. There were no executable callers
of the retired Python builder to migrate. Its older investigation references
remain historical records.

## Source, key and publication contracts

Native commands preserve the original compiler flags, object/link order and
addresses for `panda.bin.signed`, `panda_h7.bin.signed`, `bootstub.panda.bin` and
`bootstub.panda_h7.bin`. Signing preserves SETLEN=1, VERS=2 and RSA1024 with the
original raw SHA1 padding, without introducing DER DigestInfo. Public certificate
headers and firmware version bytes match the original generators.

Five build-control files are pinned by SHA256 before direct commands, including
SCons controls, firmware_version.py and sign.py. Their changes require a review
of the native contract. Shared Panda Python/SCons files remain unchanged, real
source inputs required by other consumers and the firmware content identity.
This slice does not retire jungle, shared SCons infrastructure, external ARM
compiler packaging or all Panda tools.

The complete `source_inputs` map retains the signing-key digest for unchanged-
source admission. `snapshot_inputs` describes the public snapshot and excludes
`panda/certs/debug`. Native snapshots contain neither private key bytes nor a
key symlink. The signing buffer is read once after snapshot admission and checked
against its admitted digest. All public identity, certificate and header inputs
come from the verified snapshot. Only the original comparison oracle temporarily
used a symlink to the existing repository key, removed in a finally block; no
private key bytes were copied into snapshots, fixtures, logs or receipts.

Source and tool leaves are opened with NOFOLLOW|NONBLOCK, then admitted using
opened-file metadata and a 32 MiB bound. Snapshot writes use bounded frozen bytes
whose hashes match admission. Compiler identities accept canonical SDK symlinks
before opened-file admission. Version/compiler calls use the existing owned
process-group runner with 10-second/300-second deadlines, 64 KiB/1 MiB stream
limits and a 16 MiB aggregate build-log limit. Build publication uses renameat
NOREPLACE. The operation reserve remains 25 GiB plus 512 MiB estimated growth.

Staging freezes the verified firmware and receipt bytes, checks source binding,
signatures/vectors, every destination, existing ancestor, LICENSE and provenance
before publication. All missing files are prepared and synced before the first
final name appears. No-clobber publication preserves existing matching files;
a later failure rolls back only this invocation's still-matching new inodes.
Arbitrary ancestor replacement and concurrent destination editors are outside
this ordinary-failure guarantee. Source mutation after admission is tested at
the admit/publish boundary: final bytes remain the admitted firmware and receipt.
The former internal verify/reread gap was eliminated by returning owned verified
buffers; that exact race was source-reviewed, not independently reproduced.

## Executed evidence (2026-10-14)

The unchanged original SCons/compiler/signing path and the native path both built
all four artifacts using the same pinned toolchain. Original execution used one
compiler job. Native execution was traced: 46 execve events contained no Python
or SCons executable. All 18 compiler/objcopy commands and their order matched,
normalizing only executable and snapshot safety-directory locations. All nine
compared files matched: four published firmware files, two unsigned images,
cert.h, gitversion.h and version. The unchanged original Python verifier also
accepted all four native artifacts. Firmware identity is
`DEV-1de1c7b1f187-DEBUG`.

| Artifact | Bytes | SHA256 |
| --- | ---: | --- |
| panda.bin.signed | 58160 | `9af58e47888c3d445536ce151218bdec7dec4b9fe4e56d9f5fb5ded3e3470a88` |
| panda_h7.bin.signed | 85928 | `db99218a885a7438ed4a8923fd146f2af961abbc533ab937d8c5ee310080ffeb` |
| bootstub.panda.bin | 13796 | `52ac7ba40cb123fe52ef1ba609417eb602ad924c9bff2cdc6d1009a464908670` |
| bootstub.panda_h7.bin | 18608 | `0c77563bec36e9836706d7dd18b1fcd1e4f74651bdc5b2d17950daf588cac7ed` |

- Initial compiling fail-closed stubs produced observed RED: six failures and one
  CLI negative test pass. Stage fixtures initially stopped at firmware identity;
  this was not evidence of stage-specific failure assertions.
- Fourteen focused Panda tests pass: six ownership and eight integration tests,
  in both default and `serde_json/preserve_order` modes. Independent original
  fixtures cover headers/version, signing and malformed/tampered wire inputs.
- Controlled faulty variants bypassing snapshot hashes, overwriting an existing
  empty output directory, and publishing stage files sequentially each produced
  the expected failing regression. Fixed source was restored before GREEN.
- Tests also cover source growth, symlink/FIFO leaves and tool FIFOs, failed or
  interrupted compiler cleanup, source binding, conflicts and idempotence. A real
  late license-directory write denial preserved existing firmware and published
  no new final files. This test executed as uid1000; it explicitly skips under
  root, which bypasses ordinary write permissions.
- `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools`:
  all 147 tests pass. Strict all-target Clippy passes in both JSON feature modes.
  Rustfmt (edition2021) and diff checks pass. Existing locked RSA0.9.10,
  SHA1 0.10.7 and base64 0.22.1 were reused without changing dependency versions.
- Actual staging and unchanged idempotent restaging pass in this isolated
  checkout's ignored directories: all four firmware files, exact LICENSE and
  frozen receipt match; staged file modes are0644. No vehicle writes occurred.
- Every build checked capacity first (over370 GiB available), used the coordinated
  cache, at most two build jobs, and disabled incremental compilation.

Ignored command logs, original/native artifacts, execution trace, comparison JSON
and controlled RED logs are under `.analysis/scratch/2026-10-14-panda321/`.
Full workspace/cloud validation and full-runtime/device acceptance remain separate
parent-owned gates. This conversion does not claim complete Python retirement.
