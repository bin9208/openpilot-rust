# dev-md ONNX load investigation — issue #416

Issue: [#416](https://github.com/bin9208/openpilot-rust/issues/416).
Scope: dev-md only. Original route captures and identifiers remain private.

## Observed device behavior

The two latest uploaded driving segments from the user's vehicle on October 20,
2026 were decoded with the full `openpilot.cereal.log.Event` schema. Both run
clean source `96258ca2bebbcffcb5e6b5fadb4abd07ea301d00`, `dev-md`, AGNOS
`19.8-carrot-bt1`, device family `tizi`. Each contains 30 procLog samples.
CPU percentages below use one logical core as 100%; process identity is
matched PID/startTime and executable `cmdline`, not the truncated process name.

| Measurement | Driving segment A | Driving segment B |
| --- | ---: | ---: |
| Core 0 mean CPU % | 99.83 | 99.89 |
| Core 1 mean CPU % | 99.81 | 99.81 |
| Core 2 mean CPU % | 99.72 | 99.79 |
| Core 3 mean CPU % | 93.76 | 95.11 |
| Xiaoge process CPU % | 244.80 | 226.04 |
| Xiaoge user/system CPU % | 209.81 / 34.99 | 188.78 / 37.25 |
| Main encoder CPU % | 16.51 | 16.48 |
| Auxiliary road-stream encoder CPU % | 1.52 | 1.40 |
| Maximum control publication interval, ms | 15.66 | 15.96 |
| Maximum driving-model publication interval, ms | 58.83 | 57.60 |
| Model frameDropPerc | 0 | 0 |

The nearby earlier parked/startup upload has no Xiaoge process and much less
steady little-core load; it is not a controlled ONNX A/B drive. The startup
log explicitly opens `msm_vidc_driver msm_vidc_venc`, and main/aux encoder CPU
stays approximately 18% combined in the two driving segments. This supports
hardware encoding and does not reproduce issue #344's software-encoder load.

Unique lane inferences average 1,000.25 / 1,044.10 ms (maximum 1,429.4 /
1,475.5 ms) over 60 / 58 completed results, despite
the configured 400 ms lane interval; BSD's base interval is 250 ms. Xiaoge is
the dominant additional CPU consumer. All driving control, camera, model and
pose messages in these two driving segments are valid, with zero reported
model frame drops and no false livePose input/pose/sensor checks. These data
establish overload but do not prove that missed control deadlines caused the
user's reported poor steering. Neither segment reports steering faults or
controller saturation. Active steering-angle tracking error averages 0.458 /
0.182 degrees, with maxima 5.933 / 0.904 degrees; different speed/curve contexts
prevent interpreting that comparison as an ONNX effect. Hardware/driving
acceptance remains separate.

## Source findings and repair scope

The inherited scheduler measures intervals from the pre-inference receive
time. Once compute exceeds its requested interval, another inference is
immediately eligible: there is no intentional cooling interval. Both the
pinned Python implementation and the Rust port have this overload behavior.

The Rust camera wrapper additionally copies the full NV12 allocation before
checking whether the model, cadence or BSD gate will admit the frame. This
port inefficiency wastes bandwidth at camera rate even on skipped frames.
It is not sufficient evidence for the entire one-second inference latency.

The sealed OpenCV core/DNN provider hashes match the retained provider.
Its embedded build configuration reports Release `-O3`, baseline NEON/FP16,
NEON dispatch variants, pthreads and custom HAL. Native ONNX inference selects
OpenCV's CPU backend. Missing ARM SIMD is not supported by this evidence.
OpenCV's two-thread setting does not cap two simultaneous inference callers
together to two CPU threads. Busy-wait tuning remains a separate unproved
hypothesis and is not changed by this repair.

The focused repair retains ordinary start cadence, adds one configured interval
of rest after an overrun, and avoids pixel copies until inference/snapshot
admission. It preserves model files, input dimensions, thresholds, independent
BSD-side timestamps, expiry limits, and background-core placement. An overrun
can lower detection update rate; stale results must expire by the existing
rules, never acquire an extended validity window to conceal slower inference.

## Verification and delivery

Local execution is restricted to syntax, formatting and diff review. A focused
GitHub-hosted RED/GREEN gate exercises normal cadence, one-second inference
with a 400 ms interval, dynamic intervals, BSD follow-up and recovery. Existing
pixel/model/socket/live-IPC/snapshot/reconnect and stale-publication checks,
plus affected ARM and AGNOS packaging, belong in GitHub Actions.

Implementation/cloud receipts and exact release identity remain pending.
No device was connected, installed or rebooted. No measured CPU reduction or
steering correction is claimed before the user tests the resulting candidate.

The first hosted cadence RED run is
[39450335449](https://github.com/bin9208/openpilot-rust/actions/runs/39450335449)
on source `ab83db868e31b3bdddec05869189f6e75d8431df`; it fails because the
required admission method does not yet exist (E0599), before the repair.

The remote base still contains the withdrawn self-hosted migration. For this
repair's checks, its inherited workflows are restored to their exact
`96258ca2` versions, except the owned focused-dispatch and delivery additions.
This reuses the requested [#408 rollback](https://github.com/bin9208/openpilot-rust/issues/408)
and avoids running functional validation on this workstation. The separate
CI redesign in PR #410 is left untouched. All five required checks remain
required; manual focused evidence alone cannot authorize publication.


Hosted GREEN job [118728550325](https://github.com/bin9208/openpilot-rust/actions/runs/39451883186/job/118728550325)
on `7f32501862f30606417fd1eb684bfdec1438547e` passes all four cadence tests
and all three real Git/LFS endpoint tests. Its broader native matrix and the
parallel runtime package run were canceled when review identified a new
receive-to-copy delay window; those canceled jobs are not validation evidence.

Review correction: finish admission decisions before receiving camera pixels,
then copy immediately for admitted inference or a captured snapshot request.
Only owned pixels survive subsequent parameter/lock/encoding work. A snapshot
request arriving later stays pending for the next frame, and acknowledgement
uses only its captured request number. This retains the baseline immediate-copy
exposure and does not assume a producer lease or atomic camera pixels. The
native regression must recycle the producer buffer before first consumer
access, as well as after an owned copy, and prove skipped frames stay uncopied.

Native/ARM comparisons use the existing PR jobs rather than duplicating that
matrix in the branch's focused cadence dispatch. Final package/release evidence
remains pending on the reviewed source.

## Compute-only follow-up (October 21)

The owner explicitly requires actual computation/memory reduction without
changing detection quality, models, input resolution, thresholds, cadence or
CPU placement. OFF/ON photos show substantial residual load; the system-wide
instantaneous difference is not a Xiaoge process measurement. The private
photo calculations and current acceptance status are linked from issue #416.

The follow-up candidate targets these existing redundant operations:

- BSD previously packed the entire visible NV12 frame before packing the
  configured ROI. Direct ROI extraction preserves visible Y/UV byte ordering,
  padding/gap handling, even chroma alignment and complete original frame
  validation. At 1928x1208 this avoids exactly 3,493,536 copied/allocated bytes
  per BSD preprocessing operation; it does not remove the camera ownership copy.
- Lane tensor construction normalizes one grayscale plane and copies its exact
  f32 bits into the two identical channels. Division count falls from 519,168
  to 173,056 per lane input; model input shape and all three planes are unchanged.
- Lane mask reduction visits contiguous prototype pixels while retaining the
  original channel accumulation order for each pixel. It omits rows numbered less than 52,
  which the existing acceptance boundary already rejects, and skips per-class suppression work for
  classes that were already discarded. NMS compacts its existing vector instead
  of allocating a replacement after each selected detection. Candidate order,
  confidence ties, float cancellation, NaNs and invalid bounds remain regression
  requirements; there is no reduced-precision or fused arithmetic substitution.

Frozen production references from `0682e933d3fea06e8953ace63fc521a29ee02b6e`
are retained under `rust/crates/xiaoge/tests/support/`. The hosted comparison
checks each reference against that exact Git source before use. The frozen lane
inference reference uses the frozen head rather than the candidate head.
Literal NV12 crops and bitwise grayscale-channel checks supplement existing
original-Python pixel/tensor/model/IPC comparisons on host and ARM.

Hosted RED [39535514028](https://github.com/bin9208/openpilot-rust/actions/runs/39535514028)
on `ed9f37d86f608befa1add88d641908768f09e411` fails with E0599 because the
new ROI API has not been implemented. All functional execution remains hosted.

The manual `compute_only` fast-workflow job builds the pinned, unchanged OpenCV
provider on a GitHub-hosted ARM runner. It compares both implementations on the
same inputs, warms both and alternates measurement order over nine samples.
Reports separate NV12 extraction, mask cases, normalization, BSD preprocessing
and complete real-model lane inference. Synthetic/stage speedups must not be
reported as a proportional reduction in whole-process or vehicle CPU usage.
These checks do not measure GPU headroom or establish real-device latency.
Final exact-source receipts, release identity and pending device acceptance are
recorded in issue #416 after source freeze, rather than changing source merely
to update an older snapshot's CI status.

### GPU audit and limits

The CPU photos do not report GPU occupancy. The shipped OpenCV recipe
(`rust/crates/dev-tools/src/opencv_build.rs`) sets `WITH_OPENCL=OFF`, and its
Xiaoge profile also disables DNN CUDA. The CXX network adapter explicitly
selects OpenCV/CPU. Requesting another target without a capable verified
provider does not establish acceleration. The pinned
[backend implementation](https://github.com/opencv/opencv/blob/fe38fc608f6acb8b68953438a62305d8318f4fcd/modules/dnn/src/net_impl_backend.cpp#L224)
contains its CPU fallback. CPU optimized dispatch, fusion and Winograd are
already enabled by the pinned defaults; repeating those settings is not an
established optimization.

Internal KGSL/Adreno graphs and GLES display work, AMD external-GPU execution
through the custom USB runtime, and DSP execution are distinct paths. Existing
QCOM model execution is a GPU graph path, not evidence of an available Xiaoge
DSP executor; the current ONNX Runtime adapter also selects CPU. There is no
validated GPU/DSP Xiaoge route in this candidate.

The historical full-cereal audit found 325 GPU samples equal to zero across
three retained `96258ca2` captures, with `usbgpu=false` in model diagnostics.
This does not prove present GPU headroom: `hardware-info/src/tici.rs` returns
zero when reading/parsing the internal KGSL busy counters fails, and those
counters do not describe an AMD external GPU. Today's photo device identity
and current GPU utilization are unconfirmed.

An isolated FP32 OpenCL experiment would first need a confirmed device runtime,
a different verified provider, exact model-output checks and concurrent
camera/driving/DM timing evidence. Camera buffers are not producer leases;
input/output ownership, synchronization and transfers also require accounting.
This follow-up therefore does not enable an unvalidated GPU backend, weaken
precision, or claim that the GPU is full or idle. The actual CPU memory and
arithmetic reductions above remain independent of that unknown headroom.
