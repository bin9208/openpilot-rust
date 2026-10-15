# dev-md installable candidate (#338)

The source-only handoff was incomplete. The user's 2026-10-16 correction requires
the exact-source AGNOS runtime package and public installer assets so the user
can perform the real-device test. No device is accessed by this task.

## Delivery boundary

- Keep all source changes on dev-md; do not advance dev or main.
- Build the selected Rust executables against AGNOS 19.8-carrot-bt1. Generic
  Linux ARM CI binaries are not deployment artifacts.
- Reuse external companions only with verified hashes and unchanged source
  compatibility. Rebuild Panda firmware for the latest Tesla safety header.
- Include the new MD v3 assets, selected daemon features and manager-required
  source assets. Never relabel old model or runtime binaries.
- Use the existing native manager inventory, seal, package and publication
  commands. Check clean source identity, ELF dependencies, installer admission,
  exact-SHA CI and downloaded release hashes before reporting readiness.
- Publish a prerelease named `runtime-<full source SHA>` in
  `bin9208/openpilot-rust`, without making it the latest stable release. The
  existing `ensure_runtime.sh` resolves that exact tag from the checked-out SHA.

The publisher now accepts explicit `--branch dev-md`. The default remains dev.
Both branches retain the same required-check policy; source SHA, push event,
repository, workflow, job and required-check identities are checked before and
after upload. dev-md cannot borrow a dev run, and unsupported branch names are
rejected. Ten publication tests, including the dev-md success and rejection
cases, pass. Reading dev's required-check policy does not modify dev.

## CI compatibility repairs

The first source candidate exposed stale comparison inputs and missing CI
dependencies. Repairs keep runtime behavior and strict comparisons intact:

- Fetch the two exact pinned upstream commits for source-reference loaders.
- Bind changed planner/angle/Xiaoge/stopping source helpers to those pins rather
  than retained baseline Python implementations.
- Supply the staged numerical provider path to the new RadarCAN tests.
- Keep optional native OpenCV activation explicit, matching other native crates;
  the packaged signal daemon still selects its required native features.
- Align Panda's isolated child firmware fixture, DM timing schema and manager
  stop-trace assertions with the changed producer contracts.
- Account for the 22 remaining V4L cases after source YouTube removal; each case
  retains its source/native comparison.
- Preserve the inherited unknown-road HUD display, align stale HUD test setup,
  and bind startup graphics references to the selected source.
- Apply Rust formatting and store the unpublished Wiki patch with zero context
  (`git apply --unidiff-zero`) so patch context spaces do not fail whitespace CI.

Focused local evidence includes the publication tests, two fresh Hyundai ACC
tests, three Panda composition cases, 436 Xiaoge policy cases, actual Xiaoge
OpenCV reference generation, 36 HUD tests, and 384 exact planner reference
frames. These results do not replace the final exact-SHA CI or package receipt.
The exact final SHA, Actions links, release URL and download verification are
recorded in issue #338 after publication, without changing the sealed source.

Repository-wide tool retirement remains excluded. Hardware startup, GPU
inference and driving acceptance remain user-owned and are not claimed from
host/QEMU packaging checks.
