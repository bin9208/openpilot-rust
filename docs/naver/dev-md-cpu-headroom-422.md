# dev-md CPU headroom — #422

Issue: [#422](https://github.com/bin9208/openpilot-rust/issues/422).
Implementation base: `dc95138076836ea3a5c1b6c27262be3f3b912f71`.
Branch: `codex/feat-422-cpu-headroom`, targeting `dev-md` only.

## Observed baseline, 2026-10-23 KST

The newest accessible NAS upload contains five consecutive full rlogs from
clean dev-md `f060fc423a02989acc088144ebccd7454545df65`, device family tizi,
AGNOS `19.8-carrot-bt1`. Full cereal Event decoding includes procLog,
deviceState, camera, model, pose and Xiaoge diagnostic messages. Raw captures,
identifiers and analysis scripts remain private. This source predates the
optional FP32 GPU candidate; it is not a measurement of the current branch.

Matched PID/start-time CPU deltas cover approximately 58 seconds per segment.
100% process CPU means one logical core; multithreaded values are not clamped.
In the last two segments Xiaoge consumes 158.96–163.24% (132.79–134.47% user,
26.17–28.77% system). Core 0–3 segment means span 81.17–88.14%. Logger uses
28.65–29.58%, Panda 28.20–28.46%, camera 20.84–20.91%, UI 18.67–19.76%,
card 17.10–17.17%, encoder 16.77–16.81%. Htop also consumes about 14%; it is
retained in the observed workload, not stopped to manufacture a lower result.

All five segments have 1,200 consecutive frames per camera, main fullHEVC
recording and low-resolution H264. Model frameDropPerc is zero and no livePose
input/pose/sensor checks are false. Other invalid navigation/Android messages
remain explicit in private summaries; this is not a blanket validity claim.
Snapshot Params include traffic-light mode 2, CarrotVision enabled, lane interval
400 ms, BSD interval 250 ms, DisableDM 2 and Jetlink off. Snapshot settings do
not establish every model's activity throughout a segment. Different early
and late load windows are not a controlled A/B comparison.

## Design and execution plan

Use existing inference, media and queue interfaces. Remove redundant owned
intermediates; retain identical final pixels, tensors, media and ordering.
Preserve model files, FP32, input dimensions, thresholds, cadence, expiry,
scheduling, recording quality and camera shared-memory ownership boundaries.
No changes to radar detection, lead selection, dev/main or the #296 conversion.

- [x] Locate current logs, identify source and baseline consumers, audit source.
- [x] `opencv-runtime` / `xiaoge`: compose existing BSD NV12 conversion, mask
  and resize inside native OpenCV; return only the resized image. Resize lane's
  checked strided Y-plane crop without its full-size packed temporary.
  Compare exact pixels/tensors on padded, odd-sized and portrait inputs, preserve
  original Y-only lane and full-UV BSD validation, and extend the existing same-input compute benchmark.
- [x] `loggerd/{daemon,encoder}.rs`: retain one receive allocation for immediately
  consumed encoder messages, transferring ownership only for admitted queued
  packets. Preserve FIFO/audio/rotation/error behavior and the 201-packet limit.
  Compact queued allocations and release scratch capacity above 1 MiB after a
  service drain; this cache budget must never reject a larger valid packet.
  Verify immediate reuse, independent queued storage and bounded retained cache.
- [x] `opencv-runtime/native/net.cc`: avoid the extra input clone before
  OpenCV's own synchronous copy, and bulk-copy output floats into owned Rust
  tensors. Check input release, intervening inference and exception recovery.
- [x] `signal-observer/process.rs`: transfer the completed grayscale image into
  previous-frame history. Verify consecutive-frame hashes, dimensions and error
  retention plus the existing 288-frame full-state source oracle on host/ARM;
  no tracking algorithm or temporal behavior changes.
- [ ] Review focused diffs and run local syntax/format checks only. Execute
  functional, native-memory and resource comparisons in existing sequential
  Actions CI on x86_64 and ARM binaries in QEMU/PRoot (emulated). Inspect exact-SHA required checks.
- [ ] Obtain independent review, integrate through a checked dev-md PR, verify
  merged-source checks and genuine AGNOS build/seal/publication/public installer.
  Device installation, driving and actual CPU acceptance remain owner-owned.

The release source policy admits the exact changed logger, signal and image
adapter files and focused checks. Unknown siblings and model/provider changes
remain rejected. All affected project packages already belong to the genuine
AGNOS build list and are cleaned/recompiled from the delivery source; no old
logger or OpenCV consumer binary is substituted.

## Predicted work reduction and limits

BSD avoids two native-to-Rust copies totaling six bytes per ROI pixel. Lane
avoids one packed square allocation/copy: 1,459,264 bytes for a 1928×1208 frame.
Classical signal tracking avoids cloning its grayscale history, 1,021,440 bytes
for a 1344×760 processed image. Logger avoids an allocation/free pair on steady
immediate packets that fit retained storage. These are source-level avoided
operations, not measured CPU savings; inference kernels and necessary camera
ownership copies remain. Larger gains may require separately evidenced kernel
or backend work; no percentage or sufficient future-feature capacity is promised.

The pinned OpenCV revision's [`Net::Impl::setInput`](https://github.com/opencv/opencv/blob/fe38fc608f6acb8b68953438a62305d8318f4fcd/modules/dnn/src/net_impl.cpp#L1321)
synchronously copies the input into its own `inputsData` storage before returning.
Removing the preceding clone avoids 2,076,672 copied bytes per 1×3×416×416 FP32
input without retaining the borrowed Rust pointer. Output tensors remain owned
copies, now transferred in bulk rather than one float per CXX call. The pin and
ownership regression must be reassessed when changing the OpenCV dependency.

Functional execution and performance verification are pending. The user's
local-execution restriction takes precedence over local test-first workflows:
tests are authored before changes where useful, with no fabricated local RED or
GREEN claim. Existing unchanged successful checks are not rerun locally.

The log has no separately identified signalcolord CPU sample. The classical
signal history change therefore has no measured share of this upload; mode
settings alone must not be used to claim that all inference paths were active.

## Earlier runner selection, superseded by the all-local policy below

The user explicitly selected the existing registered `bin-actions` x86-64
runner for ordinary checks, ARM cross builds and AGNOS build/package/publication.
The `dev-md-local` label selects that runner; actual native ARM execution remains
on GitHub's `ubuntu-24.04-arm`, as separately confirmed. This supersedes both the
earlier hosted-only direction and the intermediate hosted-first fallback policy.
The build-only Carrot Navi ARM job now uses the x86-64 runner's ARM cross compiler;
its locked features, target ELF check and binary hash remain, with an accurate
cross-build artifact name. The native execution matrices and RadarCAN ARM
execution job retain their real ARM runners.
Generic ARM graphics dependencies are authenticated/downloaded into a private APT
state/cache and extracted into the job's sysroot, instead of registering a foreign
host architecture or installing ARM libraries globally. The existing raylib stager
links against that sysroot; package hashes are retained with its build receipt.
The job cleanup removes the private package state. This follows APT's documented
[configuration isolation](https://manpages.debian.org/bookworm/apt/apt.conf.5.en.html)
and preserves the registered host for subsequent jobs.
Checks still run as Actions jobs; the agent's local checkout is limited to
syntax, formatting and diff review. Required checks, serial ordering, disk
estimates and per-job temporary-directory cleanup are preserved. Receipts report
the actual runner environment instead of describing self-hosted results as hosted.

PR [#494](https://github.com/bin9208/openpilot-rust/pull/494) contains the CPU
changes and this routing update. Its previous source `6441af39` passed fast
checks, mapped user docs and the integration gate in
[run 39818575855](https://github.com/bin9208/openpilot-rust/actions/runs/39818575855),
which was cancelled during workspace validation to apply the user's runner
direction. Those results do not validate the new routing commit. Independent
review found no remaining issues in the CPU and release-source changes; fresh
required-check results and native performance receipts remain pending.

The user authorized automatic follow-up through exact-source dev-md delivery.
The existing hourly follow-up now targets this task, uses the runner policy above
and reports meaningful failures, completion or required user action. Installation
on the vehicle and measured driving CPU acceptance remain user-owned.

The first local-runner PR run, `39828316661` on `12ccf5bf`, confirmed actual
assignment to `bin-actions`. Its fast checks ran 76 CI-policy tests; one existing
raylib output-isolation fixture failed because its selected shell fragment omitted
the new `cross_root` initialization. Include that allocation before expanding the
linker path. The native execution routing, private APT configuration and actual
Git/LFS regressions passed their individual checks; the full gate did not pass.
Fresh exact-source validation remains required after this fixture repair.

## Confirmed runner comparison and current execution policy

The separate #496 comparison reported three successful runs per runner: median
fast checks were 16.592 s on `bin-actions` versus 37.559 s on GitHub x64, and
clean Rust/C++ builds were 6.655 s versus 13.512 s. The #498 full AGNOS comparison
on `b538bb7d37eb3cd756ff1fe7c52fd23507142605` completed its runtime, OpenCV,
ABI, seal, offline installation and installed-launcher checks on both runners:
[bin-actions](https://github.com/bin9208/openpilot-rust/actions/runs/39825360851/job/119987118119)
took 35m47s and
[GitHub x64](https://github.com/bin9208/openpilot-rust/actions/runs/39825360851/job/119987116428)
took 44m47s, a 20.1% elapsed-time reduction. Both AGNOS jobs succeeded; the parent
run's cancellation concerns a later separate generic ARM comparison. These
measurements support CI runner choice, not this candidate's device CPU savings
or exact-source validation. The incomplete hosted generic GNU/musl comparison
has been requested cancelled by its owning task; its result is no longer a
continuation condition. The [bin-actions job](https://github.com/bin9208/openpilot-rust/actions/runs/39832976197/job/120013728323)
passed on `2ff834da04e2acaad3d63211c2b76e287679eeaa` in 55m41s, including the
GNU builds and static musl probe. This establishes that comparison job's cross
build on x64; it is neither actual ARM execution nor validation of PR #494's
different source and dependency-staging changes. At the user's final runner
selection, the hosted job remained in dependency preparation after exceeding
the local job's entire 55m41s. There is no completed paired generic timing result;
neither the delay's cause nor a pure compilation speed ratio is inferred. The
user's reported 404 errors are not independently established as its cause.

The user's latest direction fixes **all next needed jobs** to `bin-actions`,
including ARM code execution under local ARM QEMU. The earlier GitHub ARM
exception is withdrawn; neither hosted x64 nor hosted ARM is a fallback. ARM
tests must still execute ARM binaries with their existing coverage, with results
explicitly labelled as emulated rather than native hardware validation. Already
running CI and unchanged successful checks must not be cancelled or repeated
solely to change runners. The fixed free-space stop/recovery rule has been
removed from task instructions and the heartbeat; retain operation-specific
growth estimates and resource evidence. Current run `39833346797` is preserved.
Legacy numerical limits found in validation helpers are recorded for the next
necessary scoped correction; this policy update does not restart the current CI.

The runner-comparison task owns the shared dev-md routing and QEMU/PRoot
foundation in [#502](https://github.com/bin9208/openpilot-rust/issues/502) /
[PR #504](https://github.com/bin9208/openpilot-rust/pull/504). This task reuses
its verified source `77663836a95a9e2aff9aa66592d8b06ed9c87cd9`; the common PR
remains draft and unmerged. The CPU branch preserves its signal-history and
preprocessing comparisons, source receipts, private ARM graphics sysroot and
artifact captures while adopting the all-local shell and ARM target mapping.
Full validation of the combined CPU source remains necessary. These changes do
not expand device, release, protected-branch or separate #296 permissions.

## Host dependency installation failure, 2026-10-23

Source `91c4cc6751229499a7daf18246482e9a87faabd2` passed fast checks, mapped
user docs and Git update lifecycle in
[CI 39833346797](https://github.com/bin9208/openpilot-rust/actions/runs/39833346797).
Its [build-release job](https://github.com/bin9208/openpilot-rust/actions/runs/39833346797/job/120069041471)
failed during `tools/op.sh setup`: APT requested `binary-arm64/Packages` for
`noble`, `noble-updates` and `noble-security` from `archive.ubuntu.com/ubuntu`,
which returned 404. The build and subsequent runtime checks did not execute;
the three downstream aggregate gates failed closed. This is direct evidence of
the local job's package-index failure, not a CPU runtime failure or proof of the
cause of the user's separately reported hosted-runner failures.

The final setup fix reuses the common source's validated per-command scalar
`APT::Architectures=$(dpkg --print-architecture)` for native APT update/install
and queries native-architecture package status explicitly. Existing foreign
registrations and the explicit cross-build sources remain. The earlier local
temporary-config helper and its duplicate regression file were superseded by
`tools/ci/tests/test_setup_dependencies.py`; the exact-source delivery allowlist
admits the setup script and its current regression file without allowing siblings.
This does not change AGNOS SDK or native-provider build inputs.

## Shared ARM execution foundation verified; CPU validation pending

[Focused run 39853582741](https://github.com/bin9208/openpilot-rust/actions/runs/39853582741)
succeeded on the common source `77663836a95a9e2aff9aa66592d8b06ed9c87cd9`.
The real runner was `bin-actions` / self-hosted X64; guest execution used
AArch64 QEMU-user and PRoot 5.5.0 with `physical_arm_cpu: false`. Evidence covers
ARM ELF/shared-library loading via ctypes, Python 3.12.14, environment-file and
exit-code propagation, guest target visibility, ARM socketpair/SIGTERM behavior,
and the Rust disk guard. Policy 90, package 9, native guard 13, and disk guard
4 unit / 5 CLI cases passed, including the baseline rejection check.
`runtime-core --lib` had 0 passed / 1 ignored (the existing benchmark); it is
not counted as full ARM module validation. The older guest dpkg failure was
resolved in that verified common source.

The original per-module ARM tests still run through the serial PR pipeline with
`tools/ci/local-arm` and `.github/actions/local-arm`; environment/PATH command
files and target upload mappings retain host-visible artifacts. No hosted runner
fallback is used. These infrastructure results do not establish this merged CPU
candidate's functional equivalence, performance gain, full AGNOS package, or
vehicle acceptance; those gates remain part of #422's continuation.

## Workspace capacity failure and shared build-policy correction

Exact CPU source `d74ee3ab873976e31abcda4da66ac8f17cc5d9f9` passed fast
checks, mapped docs and the integration gate in
[CI 39856292777](https://github.com/bin9208/openpilot-rust/actions/runs/39856292777).
The [workspace job 120231232641](https://github.com/bin9208/openpilot-rust/actions/runs/39856292777/job/120231232641)
stopped before compilation on `bin-actions`: available space was
10,206,261,248 bytes (about 9.5 GiB), below its unchanged 12,884,901,888-byte
(12 GiB) estimated growth. This is an actual capacity shortfall, distinct from
the removed fixed 25/35 GiB policy. The later CPU/source/memory and ARM jobs
did not execute. A successful integration gate is not full runtime acceptance.

The next source adopts common #502 correction
`8afbdcf3598df7fe17f60acb2f1f0157e4d1addc`. Its
[focused local Actions job](https://github.com/bin9208/openpilot-rust/actions/runs/39873578269/job/120153608354)
passed in 126 seconds: CI policy 90, package policy 10, native guard 13,
disk-space 4 unit / 5 CLI, Python growth/Card 17, dev-tools 61 unit / 2 CLI,
model-runtime 20 and replay packaging 2. It removes remaining duplicate
fixed reserves and stale packaging expectations while retaining growth
shortfall rejection, maximum-byte handling, ownership and source/ABI checks.
CPU-specific source exceptions and required preprocessing memory coverage
are preserved when combining the branches. These shared-source results do not
validate the combined CPU source or resolve its measured capacity shortfall.

ARM transport, the shared disk-space crate and Cargo.lock are unchanged by
this correction; the focused space-only run intentionally skipped ARM.
The earlier transport evidence remains valid within its original scope.
The existing readiness workflow gains a `storage-only` diagnostic option to
report filesystem capacity, bounded directory sizes and job-temp owner records
on the registered runner. It neither deletes files nor repeats policy/ARM tests.
Use that evidence before selecting any obsolete cache for cleanup; preserve
other tasks and retain the 12 GiB workspace growth check. Combined validation,
AGNOS delivery and device acceptance remain pending.
