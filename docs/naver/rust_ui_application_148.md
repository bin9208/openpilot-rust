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
