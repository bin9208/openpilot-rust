# Shared Rust UI framework (#125)

Source: `openpilot/system/ui/lib` and `openpilot/system/ui/widgets` at
`8a3a462d`; original MIT licensing and source assets remain intact.
This is the shared framework stage of the full-runtime port, not a replacement
for `selfdrive/ui`, production process selection, or device acceptance.

## Incremental implementation

- `openpilot-ui-framework::widget`: owning child lifecycle, dynamic visibility,
  enable/touch predicates, two touch slots, parent clipping, press/release/re-entry,
  wake suppression and delayed click state. Rendering precedes input dispatch,
  matching the original frame ordering.
- `layouts::HBox`: visible-child spacing and top/center/bottom placement.
- `scroll::ScrollPanel`: source `GuiScrollPanel2`, including device-specific
  velocity history, deceleration rejection, float32 offsets, rubber banding,
  snap/bounce time constants and event/frame separation.
- `openpilot-startup-ui` continues to own the external raylib boundary and
  previously validated startup surfaces. A helper signature integration fix
  passes `false` for the existing piped-stdin child's new-session argument.

## Focused verification commands

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-ui-framework --examples -j2
python3 rust/tools/check_ui_widgets.py --binary "$CARGO_TARGET_DIR/debug/examples/widget_trace" --output "$EVIDENCE"
python3 rust/tools/check_ui_scroll.py --binary "$CARGO_TARGET_DIR/debug/examples/scroll_trace" --output "$EVIDENCE"
```

The deterministic runners import the actual source classes with only graphics,
clock, hardware identity and product UI state boundaries isolated. They retain
input, source output and native output JSON. They perform no desktop, vehicle,
Params or network mutation. The widget trace covers 800 frames; scroll traces
cover 4,500 frames across horizontal/vertical, bounce/snap and device history
lengths. Widget calls and branches must match exactly. Scroll state/touch gates
must match exactly; numeric checks allow 1e-5 offset and 1e-8 velocity to cover
cross-language floating-point arithmetic without tolerating branch changes.

## Remaining scope

Rendering/text/localization, the remaining shared widgets, Wi-Fi/NetworkManager
and application debug/record/burn-in/profile behavior are still being ported.
This document does not declare #125 or the production UI complete. Raylib,
FreeType/font shaping, graphics drivers, NetworkManager/D-Bus and kernel input
remain external boundaries. ARM/musl compilation and real AGNOS execution are
separate gates; no user device has been accessed.
