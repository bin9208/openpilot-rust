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
