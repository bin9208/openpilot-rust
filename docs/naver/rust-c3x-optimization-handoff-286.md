# C3X optimization handoff, 2026-10-14 (#286)

The user took the C3X away and requested that work be recorded in Git for
continuation on another computer. Device work has stopped. No benchmark,
paused manager, package installer, or local build remains running. GitHub
Actions continue remotely; this handoff does not cancel them.

## Start here

- Repository: `bin9208/openpilot-rust` only. Keep the unrelated original fork
  and upstream untouched.
- Work item: [#286](https://github.com/bin9208/openpilot-rust/issues/286).
- Optimization: [#288](https://github.com/bin9208/openpilot-rust/pull/288),
  already merged into dev at `f3446f65e1cbd894aba1aac7e724c12e6d89ff61`.
- Implementation and measured results:
  [fresh Params read cost](rust-params-read-286.md).
- Last verified device source/package:
  `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`. The new optimization has **not**
  been installed on the C3X. The final read attempt found the device disconnected.
- Last verified restoration: offroad, `DisableDM=2`, driver preview off,
  managers resumed, and the original installed CarrotMan running.

The code avoids metadata queries for small fresh Params reads. It does not
cache values or change setting semantics, cadence, scheduling, models, or
validity thresholds. The matched 60-second A/B/A CarrotMan comparison measured
8.1354% -> 7.3694% of one core (-0.7660 percentage points, -9.42% relative).
This is an offroad process result, not whole-runtime or driving acceptance.

## Completed evidence and remaining CI

1. Twelve Params tests, all-target strict Clippy, formatting, and original C++
   interoperability for 367 keys passed. Independent review approved the change.
2. All 65 PR checks passed for `87cc8dc1c366b2eb1015fa88acb93ae218421cbc`:
   [PR run](https://github.com/bin9208/openpilot-rust/actions/runs/38497987784).
   Its full source tree matches merged commit `f3446f65e` exactly.
3. The merged-SHA local AGNOS package pipeline passed all 13 stages and built
   all 87 executables. Normal bootstrap, manager/log-upload integration,
   ABI/model checks, inventory, sealing and package validation passed.
4. Fresh installation of the identical premerge tree under QEMU verified
   1,071 hashes and 162 ARM ELF headers with the real ARM installer/launcher.
5. Separate merged-SHA checks: [Fast](https://github.com/bin9208/openpilot-rust/actions/runs/38505272571),
   [Integration](https://github.com/bin9208/openpilot-rust/actions/runs/38505272543),
   and [user docs](https://github.com/bin9208/openpilot-rust/actions/runs/38505272594)
   passed. [Rust](https://github.com/bin9208/openpilot-rust/actions/runs/38505272602)
   was still running at handoff. Query its current result; do not reuse the PR
   result as proof of completed dev-push CI.

## Package availability matters on the next computer

The six final release assets were built on the originating workstation, under
the `runtime-package-269` managed worktree at
`.analysis/artifacts/runtime-release-f3446f65e1cbd894aba1aac7e724c12e6d89ff61/`.
They are ignored build artifacts, **not in Git and not published at handoff**.
The build receipt is under that worktree's
`.omo/evidence/device-followups-release/result-f3446f65e1cbd894aba1aac7e724c12e6d89ff61.json`.
Private captures, device identity, credentials, and temporary device-control
helpers are also not in Git. The source code and this handoff are portable;
the local native dependency cache and sealed binaries are not supplied by a clone.

Expected archive: `openpilot-rust-f3446f65e1cbd894aba1aac7e724c12e6d89ff61-aarch64.tar.gz`.

| Identity | Value |
| --- | --- |
| Architecture / OS | AArch64 / AGNOS `19.8-carrot-bt1` |
| Archive bytes | 1,156,101,708 |
| Unpacked bytes | 1,709,807,265 |
| Archive SHA-256 | `aa1540b2e9cea55f25fb8e4cde4730ff3f559f47db769f02e7aaf2fbfba71ebf` |
| Manifest SHA-256 | `d0c7ae0d4bef241e17c4902a55ed91b2f28905356ba879ef31f84dcf5c182698` |
| Installer SHA-256 | `9e7c0abc7b49a827a4dde9f858e02387e6eb9589b3e1301d1193d4ea4c7c5c33` |
| Descriptor SHA-256 | `6f7f74f913d9ed7efa1092c346b1b98e0c8165d45d267c531af97feaf89f7829` |

First check for a subsequently published release:

```sh
gh run view 38505272602 --repo bin9208/openpilot-rust
gh release view runtime-f3446f65e1cbd894aba1aac7e724c12e6d89ff61 --repo bin9208/openpilot-rust
```

If no release exists, publication still needs the originating workstation's six
verified assets, or a rebuilt and revalidated AGNOS package. A generic GNU ARM
CI artifact is not a substitute. Follow the
[distribution contract](rust-runtime-distribution-274.md),
[AGNOS validation](../rust-port/agnos-validation.md), and recorded native-provider
work under #269. Do not assume the originating workstation's absolute provider
paths exist on another computer. The corrected libyuv static provider has hash
`8c8347acee4c5b1291e4703d80264772fc8e8a3b68a41fb28e441bf8e08e14b5`;
an obsolete cache path caused the first local attempt to fail.

With the exact clean source checkout and all six assets available, use
`rust/tools/publish_runtime_release.py --root "$PWD" --assets "$RUNTIME_ASSETS"`
(first add `--check-only`). This tool requires successful exact-SHA dev-push
gates, uploads into a draft, verifies downloaded hashes, and publishes the
prerelease. Do not bypass those gates or claim publication before it succeeds.
The target tag is `runtime-f3446f65e1cbd894aba1aac7e724c12e6d89ff61`.

## Resume device work only when the user reconnects it

1. Reconfirm the connected device and offroad state. Discover the current ADB
   serial; no device identifier or SSH credential is committed here. Previous
   ADB access needed a host USB ACL, which may differ on another computer.
2. Check free space before builds, installs or large copies. Retain the project
   25 GiB reserve plus estimated growth; if below the floor, recover 35 GiB
   before new build work. Never delete routes or settings. Only the unused
   device SCons cache was removed during this task; the last observed device
   free space was about 28.7 GiB, not a current-space guarantee.
3. Inspect current source and local changes before updating. The package above
   matches exactly `f3446f65e`, not an arbitrary newer dev tip or this
   documentation-only handoff commit. Preserve generated/local files. Require
   matching source, manifest, active generation and runtime binaries.
4. Start through the normal launcher and verified installer. Check for surviving
   old recovery, web-watchdog/server and manage-athenad processes. The lifecycle
   defect [#287](https://github.com/bin9208/openpilot-rust/issues/287) remains
   open; earlier operational cleanup was not a code fix. Rediscover PIDs and
   executable paths instead of copying old PIDs from a transcript. A deliberate
   normal reboot or orderly service restart must leave no old/deleted executable
   mapping. Do not perform that transition while onroad.
5. Verify normal offroad startup and all required manager processes. The prior
   baseline had 22 processes, including two manager processes. Verify camera
   states and driver-preview inference/cadence using the full cereal schema.
   The baseline was about 19.2 ms inference and 20 Hz publication. Offroad
   monitoring-policy invalidity from missing vehicle/model inputs was expected;
   do not weaken checks to hide it.
6. Restore every preview/DM setting to its pre-test value and verify managers
   are running. Recheck the existing upload-storage connection. A health test
   alone is not evidence that a route file was uploaded. Record final source,
   release and CI URLs separately from physical and driving acceptance.
7. Update #286 with final installation results. Keep #287 open until its actual
   lifecycle acceptance criteria are met. Do not infer driving acceptance from
   the completed offroad profiling.

The next session should resume from publication and installed-candidate
verification, not redo the completed Params implementation or profiling.
