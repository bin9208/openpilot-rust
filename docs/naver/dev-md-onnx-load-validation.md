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
