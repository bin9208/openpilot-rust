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
