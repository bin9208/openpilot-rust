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
- [ ] `opencv-runtime` / `xiaoge`: compose existing BSD NV12 conversion, mask
  and resize inside native OpenCV; return only the resized image. Resize lane's
  checked strided Y-plane crop without its full-size packed temporary.
  Compare exact pixels/tensors on padded, odd-sized and portrait inputs, preserve
  original Y-only lane and full-UV BSD validation, and extend the existing same-input compute benchmark.
- [ ] `loggerd/{daemon,encoder}.rs`: retain one receive allocation for immediately
  consumed encoder messages, transferring ownership only for admitted queued
  packets. Preserve FIFO/audio/rotation/error behavior and the 201-packet limit.
  Compact queued allocations and release scratch capacity above 1 MiB after a
  service drain; this cache budget must never reject a larger valid packet.
  Verify immediate reuse, independent queued storage and bounded retained cache.
- [ ] `opencv-runtime/native/net.cc`: avoid the extra input clone before
  OpenCV's own synchronous copy, and bulk-copy output floats into owned Rust
  tensors. Check input release, intervening inference and exception recovery.
- [ ] `signal-observer/process.rs`: transfer the completed grayscale image into
  previous-frame history. Verify consecutive-frame hashes, dimensions and error
  retention plus the existing 288-frame full-state source oracle on host/ARM;
  no tracking algorithm or temporal behavior changes.
- [ ] Review focused diffs and run local syntax/format checks only. Execute
  functional, native-memory and resource comparisons in existing sequential
  GitHub-hosted CI on x86_64 and ARM. Inspect exact-SHA required checks.
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
