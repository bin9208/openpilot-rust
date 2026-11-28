# Experimental Android Jetlink app

The target is a Galaxy Tab S9 Ultra with 12GB RAM (Android 15 or later) and comma 3X. The APK targets arm64 Android 9+, but compatibility and measurements on the target device require separate testing. This is a model/link validation stage, not approval for vehicle use.

## Install and import

1. Check the supplied APK SHA-256, then open it on the tablet. Your file app may need permission to install apps.
2. Open **Carrot Jetlink → Model → Import model package ZIP**. Weights are not bundled in the APK. The app verifies the archive before storing it privately; import needs temporary storage as well as model space.
3. The ZIP must contain `manifest.json`, `model.onnx`, then optional `benchmark.frames.bin`, in that order. Paths, sizes, SHA-256, tensor shapes and state mappings are checked. Use the stateful CTV3 package preserving the original model bytes. Queued models are refused while numerical conformance is unverified.
4. CPU is the initial baseline. NNAPI requests an execution provider; it does not prove full NPU execution. Preparation may fail or some operators may run on CPU.

Do not uninstall or clear app data during updates. Install an APK with the same signing key over the existing app to preserve models. CI debug APKs may have a different temporary signature each run; do not mix them with the locally distributed APK for updates.

## Parked benchmark

Choose **Benchmark → Run 100-frame benchmark** and keep the screen open. After preparation and warmup, the app measures actual ONNX Runtime inference using repeated synthetic package inputs or black images with fixed context. It shows mean/p50/p95/p99/maximum and the number of frames over 50ms. Percentiles cover the last 2,000 samples; mean, maximum and misses cover the full run.

**Save numerical comparison ZIP (after benchmark)** exports the actual outputs for the first input sequence. Package inputs are limited to 100 frames; longer sequences are rejected at import. Extract it on a PC and use `tools.jetlink_model.compare` against a PC recording of identical inputs. **Diagnostics → Save diagnostic report** exports device/model/requested provider/performance/thermal information. Measurements retain the model/provider selected when the run began, even if later selections change. Health readings are sampled at export. Diagnostics contain no camera images or location.

These timings exclude C3X camera processing and the complete USB round trip. An APK build or synthetic-input result does not establish tablet performance, 30-minute thermal stability or vehicle acceptance.

## USB and stopping

The C3X needs a branch that contains Jetlink; the app alone is not enough. There is no setting to turn it on or off: the `Jetlink mode` (0/1/2) of earlier guides is gone. Connect a USB 3 data cable, wait for the C3X to prepare the Jetlink vendor gadget (1209:0001), then press **Status** → **Connect comma USB** in the app and allow USB access. The app uses a connection service with a notification. The model used is the one loaded in the app.

After denied permission or cable removal, reconnect manually. Use **Stop** in the app or notification to stop the service. It does not automatically reconnect after a reboot/process death. Thermal status SEVERE or above stops work; cool the device before retrying. Memory/model errors appear on the status page. An engine reporting `ready` does not authorize driving.

Pending device checks: install/rotation, import, CPU and NNAPI output comparison, USB denial/detach/reconnect, 30 minutes of continuous use and thermal state, and disengagement when a phone frame is lost. Field criteria in #44/#45/#46/#47 remain open until evidence is available.

## How the C3X moves to the phone model and back

When the phone is connected and its model is ready, the C3X moves to the phone model by itself once all of these hold:

- The internal model has produced at least three outputs and the camera calibration has been received.
- Vehicle state, driving state and the control command are valid and at most 0.25 s old.
- The car is fully stopped, or the driver is holding the wheel.

After the move these conditions are not checked again while driving.

While the phone model is used, the C3X's internal model does not run. The C3X shrinks the camera images to the model's input size, sends them to the phone and publishes the phone's answer. It waits up to 0.15 s per frame. A frame slower than 0.05 s is still used and is logged as `Jetlink slow frame`.

If the phone keeps answering later than the frame period (0.05 s), the frame rate falls below 20 per second. Below 16 a communication-issue warning appears and openpilot disengages. This happens when the phone gets hot and its inference slows down. Cool the phone.

When a phone frame is lost (over 0.15 s, cable removed, invalid answer):

- From that frame on the internal model answers again, starting from an empty history.
- A communication-issue warning is raised for 5 s and openpilot disengages, so that the model never changes silently under control.
- After those 5 s the C3X moves to the phone model again when the conditions above hold (stopped, or wheel held). Nothing needs to be pressed.

The USB gadget is not reconfigured while driving. If kernel I/O is stuck and cannot be released, restart the device while parked. There has been a case where the USB link dropped physically during a drive and did not come back; the cause is not known yet.

## Checking the logs

- In the drive log (rlog), `drivingModelData.jetlink` and `modelV2.jetlink` carry, per frame, the source (`native`/`jetlink`), the phase (`ACTIVE`/`PREPARING`/`READY`/`LOST`) and the time taken (`executionMs`).
- `runtimeTiming` events in the same log carry per-second summaries: from `modeld`, `jetlink_warp_ms`, `jetlink_handoff_ms`, `jetlink_roundtrip_ms`, `jetlink_parse_ms`; from `jetlinkd`, `usb_exchange_ms` and the phone's own `server_gpu_ms`, `server_queue_ms`, `server_total_ms`. `usb_exchange_ms` minus `server_total_ms` is the time spent on the USB transfer.
- Link state and failure reasons are in `/dev/shm/carrot-jetlink.json` and `/dev/shm/carrot-jetlink-model.json`.

The figures in this guide are the values in the code. Driving on the phone model has not been confirmed enough on a vehicle yet.
