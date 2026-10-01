# Native product UI application (#148)

Issue [#148](https://github.com/bin9208/openpilot-rust/issues/148), full runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1), shared framework
[#125](https://github.com/bin9208/openpilot-rust/issues/125).
Original `openpilot/selfdrive/ui` source and MIT provenance are retained.

## State and device policy checkpoint

`rust/crates/ui-application` begins the product application with UIState,
last-complete timed Params snapshots, slow settings/CarParams refresh and
brightness/wakefulness policy. Typed cereal extraction subscribes to the same
28 services; no production process selection changes here.

The unchanged UIState/Device class bodies execute in the host oracle with
owned Params, message, clock and hardware-effect seams. Three scenarios cover
big, Mici and PC behavior across 2,400 frames: ignition/engagement, stale and
missing Panda/camera updates, first-frame callbacks, source transition order,
Params retry intervals and CarParams retention, onroad brightness, automatic
brightness ratio, worker-busy suppression, timeouts and display-power effects.
Discrete states/actions and ordered Params reads match exactly. Filtered f64
brightness/time values use a fixed 1e-11 absolute bound, chosen for source
Python/NumPy versus Rust f64 arithmetic; rounded brightness is exact.

```sh
# Check the 25 GiB reserve plus estimated build growth before compilation.
CARGO_INCREMENTAL=0 cargo build --manifest-path rust/Cargo.toml \
  -p openpilot-ui-application --example state_trace --locked -j2
python rust/tools/check_ui_state.py \
  --binary "$CARGO_TARGET_DIR/debug/examples/state_trace" --output "$EVIDENCE/state"
```

Local checkpoint evidence: `.omo/evidence/ui-application-148/state-stage1/`,
`state-stage1.log`, `state-native-build.log`, `state-clippy.log` and
`state-ruff-final.log`. This is an intermediate policy checkpoint, not #148
completion. Actual UI process composition, both product layouts/settings,
async API/settings outcomes, camera/VisionIPC/EGL rendering, onroad layers,
visual/live-input/recording gates and native display placement remain underway.
Separate soundd/feedbackd services and the installer are outside this UI crate.
External raylib, graphics/font/codec libraries, NetworkManager and hardware
interfaces remain explicit; no device, C3X, NAS or account was accessed.

Docs-Not-Needed: language conversion preserves settings behavior and production
selection; no public guide behavior changed.

## Product services and widgets checkpoint

The native crate now owns ordered Params writes, source DisplayScheduler
sweeps, Mici circle/big/scroll/multi/bool buttons, Prime/setup/Carrot Web screens,
and UI authentication cache policy. QR segmentation, error correction and
source mask scoring are preserved over the native qrcodegen dependency.
Dynamic GPU textures release on the renderer thread; destruction after the
renderer is gone does not call GL. The external raylib/driver remains native
and is not claimed to have been rewritten or fully sanitizer-instrumented.

The owned host evidence under `.omo/evidence/ui-application-148/` contains:

| Scenario | Invocation | Binary observable / artifact |
| --- | --- | --- |
| UIState/Device regression | `check_ui_state.py --binary .../state_trace --output .../state-stage2` | 2,400 source/native frames; `state-stage2.log` |
| Display policy | `check_ui_scheduler.py --binary .../scheduler_trace --output .../scheduler-final` | 160 exact ordered sweeps and own-process policy readback; `scheduler-final/result.json` |
| QR encoding | `check_ui_qr.py --binary .../qr_trace --output .../qr-final` | 64 complete source matrices and masks; `qr-final/result.json` |
| Mici controls | `check_ui_product_buttons.py --binary .../mici_buttons --output .../buttons-final --display :125` | 280 interaction frames and eight exact pixel scenes; `buttons-final/results.json` |
| Prime/setup/Carrot Web | `check_ui_product_widgets.py --binary .../product_render --output .../product-final3 --display :125` | All 14 English/Korean screens have zero differing pixels; `product-final3/results.json` |
| Token policy | `check_ui_api_tokens.py --binary .../api_tokens --output .../api-tokens` | 26 signed JWT claim comparisons, RSA preference/EC fallback, cache/time boundaries; `api-tokens/results.json` |
| Texture ownership | `check_ui_qr_texture.py --binary .../qr_render --output .../qr-texture-final --display :126` | Exact pixels/lifecycle states and late-drop successful exit; `qr-texture-final/result.json` |
| Rust ownership checks | `cargo test -p openpilot-ui-application -j2` | Cache retry/acknowledgement and 100 FIFO writes/worker restart; `ownership-final.log` |
| Adapter sanitizer | `check_startup_ui.adapter_asan(Context(...))` | Repeated/invalid texture release accepted without ASan finding; `resources-asan.log`, `resources-asan/asan-build.log`, `resources-asan/asan.log` |

Native binaries are from the coordinated inactive target cache. Renderer
checks use the pinned native raylib library and owned Xvfb displays. Source
oracles load the original classes with isolated Params/clock/IPC seams;
synthetic authentication keys remain only in ignored local evidence. JWT
claims and signatures are compared, not encoder-specific JSON member order.
The Korean Prime check-mark glyph and elided text are inherited from the
source rendering, not redesigned in this conversion.

This is still an intermediate checkpoint: it does not compose the complete
UI application or fulfill the #148 handoff. Home/sidebar/onboarding/settings,
network/API workers, onroad camera/model/HUD/alerts and end-to-end process
lifecycle remain in progress. Native eGPU backend completion is tracked in
[#154](https://github.com/bin9208/openpilot-rust/issues/154).

## Pairing and API worker checkpoint

Both pairing layouts, their distinct QR styles and the big-display SSH action
are now native. All 22 product render scenes match the original pixels exactly
in English/Korean (`product-ssh/results.json`). The small pairing label retains
its source English string and tight line spacing even under Korean font selection.
Background callbacks and rendering share the current translation catalog.

Prime and Firehose preserve their authenticated GET sessions, offroad/awake
polling gates, 5/30-second fetch cadence and 10-second/unbounded request timeout
policies. SSH retains its 15-second timeout, ordered username/key writes,
error clearing before callback and localized outcomes. Polling stop wakes idle
workers and preserves the source one-second join limit for active requests;
any outstanding request owns its data until completion. These are implemented
services; application-wide gate updates and lifecycle composition remain part
of the pending full UI integration.

| Scenario | Invocation | Observable / artifact |
| --- | --- | --- |
| GET transport | `check_ui_api_http.py --binary .../api_get --output .../api-http` | Exact response text/status and selected request headers, cookies, cross-port auth removal, both deflate formats, redirect limit, progressing body and timeout; `api-http/results.json` |
| Prime/Firehose | `check_ui_api_services.py --binary .../api_services --persist .../synthetic-persist --output .../api-services-final` | Original class bodies and real native Params/loopback HTTP: initial typed values, missing/unregistered identities, changed/unchanged status, malformed/nonfinite/arbitrary-integer JSON and exact stored JSON bytes; `api-services-final/results.json` |
| SSH workers | `check_ui_ssh_fetch.py --binary .../ssh_fetch --output .../ssh-fetch-final` | Nine real worker scenarios, including the actual 15-second timeout, Unicode whitespace, callback Params state and Korean errors; `ssh-fetch-final/results.json` |
| Polling lifecycle | `cargo test -p openpilot-ui-application -j2` | Onroad/asleep requests suppressed, offroad/awake execution, idle worker closure release, repeated stop; `services-tests.log` |
| Pairing/SSH screens | `check_ui_product_widgets.py --binary .../product_render --output .../product-ssh --display :125` | 22 exact source/native screens; `product-ssh/results.json` |
| Native ownership | `check_ui_native_resources.py --target .../debug --raylib .../raylib-host --output .../resources-stage3 --display :126` | Actual clear-color readback, idempotent release and ASan; per-compiler disk checks and exact invocations under `resources-stage3/` |

`services-clippy.log` and `services-ruff-final.log` capture the package/code
checks. The shared registration decoder is only made public for reuse; its
conversion policy is unchanged. No real account, NAS or vehicle endpoint was
contacted. These checkpoints do not close #148 or replace complete runtime,
settings/onroad/camera and normal-startup/log-upload acceptance.

## Toggles and Firehose checkpoint

Both display layouts now implement the original toggle panels and Firehose
information/scrolling views. Toggle confirmation, engaged/parameter locks,
CarParams-dependent experimental/alpha availability, restart requests,
personality synchronization and compact debug actions retain source behavior.
Typed integer Params preserve Python negative-index behavior for compact
personality controls. Firehose uses the existing native polling service.

| Scenario | Invocation | Observable / artifact |
| --- | --- | --- |
| Full product regression | `check_ui_product_widgets.py --binary .../product_render --output .../product-stage4 --display :125` | 30 English/Korean source/native scenes, all zero differing pixels; `product-stage4/results.json` |
| Interactive settings | `check_ui_settings.py --binary .../product_render --output .../settings-trace2 --display :125` | 20 scenarios, 1,600 exact Params/action/personality frames and zero final pixel differences; `settings-trace2/results.json` |
| Package regression | `cargo test --manifest-path rust/Cargo.toml -p openpilot-ui-application --locked -j2` | Four cache/FIFO/poller tests pass; `settings-tests-final.log` |
| Static checks | `cargo clippy ... -p openpilot-ui-application --all-targets -- -D warnings`, `ruff check` | Successful checks; `settings-clippy.log`, `settings-ruff-final.log` |

Evidence is under `.omo/evidence/ui-application-148/`. The interaction oracle
executes the original refresh methods with real cereal CarParams/selfdrive
messages and owned Params. It injects confirmation results at the callback
boundary; this is not a claim of full modal gesture coverage. Native asynchronous
Params writes finish at each comparison barrier, so traces establish policy
outcomes, not background-thread timing equivalence. Root application composition,
device/network/software/developer settings and the onroad surface remain pending.
No production selection or device-test readiness is claimed.

## Device settings and dialogs checkpoint

Both device panels now retain the original identity, pairing, updater, reset,
power, language and regulatory behavior. Compact slide/text dialogs preserve
keyboard geometry, held backspace, text overflow and dismissal callbacks.
Calibration descriptions decode original cereal Params, and reset/power actions
recheck engagement at confirmation. Language options retain JSON insertion order.
The shared full-width dual-button action follows its source width after the first
render; this preserves the source power-row placement.

| Scenario | Invocation | Observable / artifact |
| --- | --- | --- |
| Device/settings/regulatory/language | `check_ui_device.py --binary .../product_render --output .../device-final --display :125` | 64 scenarios / 5,580 exact Params/effect frames and zero final pixel differences; `device-final/results.json` |
| Compact dialogs | `check_ui_product_dialogs.py --binary .../product_render --output .../dialogs-final --display :126` | 20 scenarios / 1,200 callback, text, candidate and dismissal frames; every final image matches; `dialogs-final/results.json` |
| Previous product screens | `check_ui_product_widgets.py --binary .../product_render --output .../product-stage5 --display :126` | All 30 English/Korean screens still match exactly; `product-stage5/results.json` |
| Toggle/Firehose interactions | `check_ui_settings.py --binary .../product_render --output .../settings-stage5 --display :126` | All 20 scenarios / 1,600 frames still match; `settings-stage5/results.json` |
| Package and static gates | `cargo test` and `cargo clippy --all-targets -- -D warnings` for both UI crates, focused Ruff | Five package tests and static gates pass; `device-tests-final.log`, `device-clippy-final.log`, `device-ruff-final.log` |

Artifacts are under `.omo/evidence/ui-application-148/`; `stage5-receipt.json`
records full commands and the tested binary digest. `DESIGN.md` in the product
crate records the existing source contract and inherited constraints.
The host checks capture updater requests and use private Params; they do not
signal a real updater or power down a host. Device-page confirmation results
are injected at their callback boundary, while the separate dialog scenarios
drive actual gestures. Unopened driver/training pages are captured as factory
requests, not claimed as rendered or integrated. Normal application composition,
updater signal adapter, remaining settings/onboarding/home/onroad/camera and the
complete runtime startup/upload gate remain pending.

## Intermediate developer/software checkpoint (2026-10-01)

Both developer layouts now use the original release, offroad, longitudinal and
mutual-toggle policy. Compact SSH entry preserves its original whitespace and
clock behavior; the large layout retains its trimmed username and separate
confirmation. The software panel retains release notes, branch ordering, the
strict ten-second idle wait, install/uninstall writes and the original timestamp
presentation. These are existing settings ported without a new setting or guide
change.

The native updater adapter uses the running, positive `updated` PID from
`managerState`, verifies the expected `openpilot-updated` executable and process
start time, and uses an owned pidfd where available. Its legacy-kernel fallback
rechecks identity before signalling; it cannot eliminate the final check-to-kill
race. Missing or changed identity reports an unavailable outcome. The panel's
waiting state is not a fabricated updater-success response. Normal application
wiring remains pending.

| Scenario | Binary observable | Captured evidence |
| --- | --- | --- |
| Developer release/offroad/CP gating, mutual toggles, alpha confirmation, ADB/SSH/debug, and owned HTTP key success/empty/404/pending responses in English and Korean | 52 real original/native render scenarios; identical per-frame Params/effects and zero final differing pixels | `stage6-developer-final/results.json`, paired PNGs/traces and `http-requests.json` |
| Software release notes, check/download states, timeout, install/uninstall, branch selection, malformed Params and elapsed-date boundaries | 52 real original/native render scenarios; identical effects and pixels, including unpadded early-year dates | `stage6-software-final/results.json` and paired PNGs/traces |
| Owned updater processes: actual signals, wrong identity, invalid/stopped PIDs, exec replacement and restart | 17 managerState/pidfd/legacy-path cases pass using real owned children | `stage6-updater-signal/results.json` and child/build logs |
| Params TIME parsing | 17,258 CPython/native conversions agree for calendar/week/basic dates, offsets, fractions, invalid and mutated strings | `stage6-datetime/results.json` |
| Prior toggle/Firehose, device, regulatory/language and compact-dialog interactions | 20 + 64 + 20 scenarios retain exact original/native pixels and traces | `stage6-settings-regression/`, `stage6-device-regression/`, `stage6-dialogs-regression/` |
| Prior product widgets | 30 original/native screenshots remain pixel-identical | `stage6-widgets-regression/results.json` |
| Package and static checks | Package tests, Clippy with denied warnings and Ruff pass | `stage6-tests.log`, `stage6-clippy-final.log`, `stage6-ruff-final.log` |

The timestamp compatibility investigation used CPython's
[`_datetimemodule.c` parser](https://github.com/python/cpython/blob/v3.12.3/Modules/_datetimemodule.c),
then checked behavior directly against the installed pinned Python. The pure
Python fallback parser alone does not establish C-parser equivalence.

Evidence remains under `.omo/evidence/ui-application-148/`. SSH uses only an owned
loopback server and fictitious keys; updater signalling targets only owned test
children. Modal results are injected at the policy callback boundary while the
separate compact-dialog regression drives actual gestures. No vehicle, C3X,
account, real updater, host power operation or production selection is involved.
This is still an intermediate #148 checkpoint: network/eGPU/settings composition,
home/onboarding/sidebar/onroad/camera, native application startup and the complete
runtime startup/upload gate are unfinished.

## Compact network recovery checkpoint (2026-10-01)

The native compact Network and Wi-Fi pages now retain the source scan-card
ordering, connection/authentication/forget callbacks, signal/security icons,
connection animations, metering/tether controls, cellular visibility and APN
trimming. Shared scroller moves use stable widget IDs while painting; the Wi-Fi
session refreshes the synchronous command snapshot before returning. The public
Wi-Fi widget exposes its tick for onboarding/application lifetime ownership.

The resumed real raylib run covers 46 English/Korean scenarios and 3,740 frames.
Every captured pixel and per-frame transport/Params/effect trace matches the
unchanged Python source. All paired images, scene inputs and JSON traces are in
`.omo/evidence/ui-application-148/network-resume/`; the exact command, binary hash,
coverage and limits are in `network-resume-receipt.json`. The corresponding
package/framework tests are captured in `network-tests.log`. A separate existing
30-scenario widget regression uses `network-widgets-regression/`.

Visual inspection retains source behavior, including English-font fallback for
Korean SSIDs, English compact labels and source wrapping under the Korean font.
This verifies an owned transport boundary, not real Wi-Fi or system D-Bus.
eGPU, settings composition, remaining camera/onroad and full application/runtime
startup remain open; this checkpoint is not a device-test candidate.

## Stage 7: settings roots, eGPU controls and owned camera preview (2026-10-01)

Both settings roots now compose the existing native panels. The large root keeps
panel instances and the shared resolved Params namespace across hide/show; the
compact root emits the original page selections and gates Pair/eGPU cards using
the same state. The shared label helper retains Python's double precision until
passing final coordinates to raylib, fixing the observed 1-channel Network-label
pixel differences without changing layout rules.

Large/compact eGPU panels use the native USB status/link/check/manifest boundary.
Checks own their worker, reject duplicate starts and cancel/join on destruction.
The source status priority, onroad disable rule and compile confirmation are
preserved. These UI tests use owned synthetic USB paths/probes; the actual eGPU
probe implementation and device/model execution remain separately tracked in
#154 and are not established by this UI checkpoint.

The camera widget owns a real VisionIPC client and NV12 textures, preserves the
last frame between arrivals, switches only after a target frame arrives, and
reconnects using the original transition/retry rules. Host shaders retain the
large/compact conversion, driver enhancement and driver flip. The target EGL
path owns duplicated frame descriptors and images independently of the client
borrow. Its ABI lifecycle is tested with a private fixture, not AGNOS hardware.

Compact driver preview includes both normal and onboarding setup variants,
face/eye/glasses overlays, source filters and dmoji geometry. `Preview::new`
constructs the setup variant for Home; `with_camera(..., setup)` permits an owned
server in QA. `driver_orientation` refreshes selected-driver metadata before
Tutorial positioning without advancing filters. Show/hide retains the original
Params and interactive-timeout behavior. The temporary offroad `selfdriveState`
feed uses the registered service queue size and yields ownership to a later
normal daemon publisher. Ordinary publishers retain their exclusive lock; an
existing normal publisher cannot be displaced by the transient UI constructor.
The current DriverMonitoringState schema has no events list, matching the
source's absent-events fallback and default no-alert-sound publication.

Evidence under `.omo/evidence/ui-application-148/`:

| Scenario and invocation | Binary observable | Captured artifact |
| --- | --- | --- |
| `check_ui_egpu.py` | 72 EN/KO cases, 2,320 exact source/native frames; effects, call counts and compile confirmation agree | `egpu-final2/results.json`, per-case frames/traces, `egpu-tests.log` |
| `check_ui_settings_root.py` | 38 EN/KO cases, 1,640 exact frames and identical panel/page/Params outcomes | `settings-root-final/results.json` |
| `check_ui_camera.py` | 10 large/compact cases, 240 exact real VisionIPC frames and identical frame/stream metadata | `camera-all/results.json` |
| `check_ui_camera.py --filter lifecycle` | Both layouts retain frames through pauses and recover after server restart/onroad/offroad transitions; 48 exact frames and metadata | `camera-lifecycle/results.json` |
| `check_ui_camera.py --driver` | Five setup/normal preview cases, exact frames including fractional crop rectangles; hide/reshow Params and timeout checks; live owned msgq alert records stop on IsOnroad | `driver-transient-final/results.json`, `*-messages.json` |
| `check_ui_egl.py` with borrowed and owned modes | Seven cases each, exact source/native ABI traces, zero descriptor delta, owned image retains context after original owner drops | `egl-camera-borrowed/results.json`, `egl-camera-owned/results.json` |
| `cargo test` for UI application/framework/msgq | Worker/Params tests, duplicated VisionIPC descriptor bytes and transient-to-daemon publisher replacement pass | `transient-tests.log` |
| `check_ui_native_resources.py`, `check_msgq_sanitizers.py` | ASan/UBSan pass; short/released planes and overflowing dimensions rejected, normal publisher preemption preserved | `camera-asan/`, `msgq-transient-asan.log` |
| `check_ui_product_widgets.py` | 30 existing EN/KO screens still match exactly after the shared label precision correction | `stage7-widget-regression/results.json` |

`stage7-receipt.json` records exact invocations, artifact paths and binary digests.
Contact sheets retain the observed original elision and untranslated compact
English copy in the Korean locale. Captured live alert comparison uses the stable
subscriber window: msgq can discard the first packet during publisher-reset
resynchronization, so these records do not claim to capture every constructor
publication. Queue-replacement behavior is separately exercised at the transport
boundary.

Home/onboarding integration, the large driver dialog, remaining onroad HUD and
alerts, and the final application startup/recording/shutdown path remain open.
No production selection, C3X connection, measured CPU saving or first device-test
readiness is claimed.
