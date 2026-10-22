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

The C3X needs a branch containing the Jetlink integration; the app alone is insufficient. With the vehicle stopped and cruise, lateral and all control disengaged, select **External GPU model (1)** in Carrot Web `System → Jetlink External Model → Jetlink mode`. This requests actual external model output as soon as its live session is ready and compatible. There is no separate Shadow option or 1 → 2 activation step. **Off (native model, 0)** remains the default, and manager restart/reboot resets it to 0. Old saved values 1 and 2 both request external output; the web UI presents 2 as the same external choice without writing it back merely on display.

Use a USB 3 data cable, choose **Status → Connect comma USB** in the app, and grant USB permission after the device prepares its Jetlink USB connection. A foreground service shows an ongoing notification. Direct Android USB bulk does not require a host IP address. The web card shows an IP only when the host reports one; connection/model errors remain visible. Choosing CPU or NNAPI in the app selects its inference backend and does not enable Jetlink on the C3X.

Jetlink temporarily releases the stock AGNOS ADB USB gadget when it shares the controller. Saved ADB settings remain intact; disabling Jetlink or a preparation failure restores ownership according to the current setting. This can disconnect USB ADB. Unknown gadgets and eGPU ownership are never forcibly released.

After denied permission or cable removal, reconnect manually. Use **Stop** in the app or notification to stop the service. It does not automatically reconnect after a reboot/process death. Thermal status SEVERE or above stops work; cool the device before retrying. Memory/model errors appear on the status page. An engine reporting `ready` does not authorize driving.

Pending device checks: install/rotation, import, CPU and NNAPI output comparison, USB denial/detach/reconnect, 30-minute latency/thermal testing, full C3X 50ms latency and control disengagement on loss. Selecting the test candidate does not establish those results. Field criteria in #44/#45/#46/#47 remain open until evidence is available.

## C3X selection and validation boundary

External selection uses current external predictions for model publication after the stopped/disengaged source switch. If selected while moving or engaged, it waits for a stopped/disengaged opportunity without requiring another setting change. Native inference continues every frame during external activation to preserve fallback history. A separate 50ms local communication deadline isolates a stalled USB-owner process. The full budget runs from C3X warp through external response/parsing to immediately before publication, unlike the standalone app benchmark.

Explicit external selection does not require a historical `JetlinkValidation` acceptance profile. Live checks still require a supported session/model contract, a matching executed-artifact SHA-256, camera/control readiness, finite correctly shaped output, the current frame/session identity and the complete 50ms deadline. An invalid session or artifact keeps native selected with an error. This opt-in is for the actual test candidate, not a claim of device or driving acceptance.

The logged `validated` flag remains a separate historical-evidence indicator. It stays false when there is no matching approved profile, even while external output is active. Existing profiles still bind model/device/backend/app/runtime identity, numerical comparison and at least 30 minutes of latency/loss evidence. Changing the device/backend/version can invalidate that evidence without substituting a fabricated approval. Benchmark success alone never creates it.

The first active timeout/detach/wrong-frame/NaN discards external output and latches loss. It enters the external-model-loss no-entry/soft-disable path; AlwaysLateral-only steering cannot bypass loss. Native success, reconnection, stopping or changing between legacy values 1 and 2 does not clear it. Stop, disengage all control, select **Off (0)** to acknowledge, then select **External GPU model (1)** again. Restart after an active loss retains the acknowledgement requirement. Old frames are never relabelled fresh.

Recover a failed USB connection while parked: toggle 0 → 1 and reconnect the app. Stuck kernel I/O may require a parked device restart. `JetlinkStatus` describes preparation/errors; `modelV2.jetlink` and `drivingModelData.jetlink` logs carry source/loss/session/timing. C3X QCOM device builds, physical USB speed, camera pixel comparison and drive acceptance remain separate validation work.

The Android package workflow above requires the supported original model bytes. The native `dev-md` runtime also pins supported App-selected model contracts per session; a changed/unloaded App model ends that session and an onroad model change is not adopted automatically. Preparation rejects a missing or mismatched executed-artifact hash. Matching a source name or tensor shapes cannot substitute for matching the selected artifact.
