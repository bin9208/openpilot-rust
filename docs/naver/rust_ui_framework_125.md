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

## Text, assets and control rendering stage

The shared crate now includes Label/UnifiedLabel wrapping, ellipsis, emoji runs,
marquee timing, shimmer, all button palettes, radio/icon buttons, Toggle, Icon,
and outlined/shadowed text helpers. It retains the source assets, spacing,
font scales, colors, animation constants and rendering-before-input ordering.
The #117 adapter adds checked RGBA upload, tinted arbitrary textures, circles,
gradients, lines, image flip/resize options and all source font weights.

Color emoji executes entirely in Rust: locked `rustybuzz`/`ttf-parser` shapes
NotoColorEmoji; `png` decodes its bitmap strike. The source fixed 128-square
canvas, 109-pixel strike, baseline and alpha arithmetic are preserved. The
integer alpha conversion follows the source dependencies' behavior:
[FreeType PNG alpha conversion](https://github.com/freetype/freetype/blob/VER-2-14-3/src/sfnt/pngshim.c)
and [Pillow glyph conversion](https://github.com/python-pillow/Pillow/blob/12.3.0/src/_imagingft.c).
No Python interpreter or Pillow is called by the native runtime.

`multilang` parses the same PO files, preserves untranslated/empty fallbacks,
plural selectors and Params-backed language selection. Focused commands:

```sh
python3 rust/tools/check_ui_emoji.py --binary "$CARGO_TARGET_DIR/debug/examples/emoji_raster" --output "$EVIDENCE"
python3 rust/tools/check_ui_translations.py --binary "$CARGO_TARGET_DIR/debug/examples/translation_trace" --output "$EVIDENCE"
python3 rust/tools/check_ui_render.py --binary "$CARGO_TARGET_DIR/debug/examples/ui_render" --output "$EVIDENCE/render" --display "$DISPLAY"
```

Seven actual-source emoji rasters match every pixel; all 12 PO catalogs and
5,733 plural cases match. Six source/native Xvfb scenes match every pixel and
state snapshot: wrapping, emoji, marquee, shimmer, controls and Korean text.
The independent parent review accepted controls, emoji and Korean screenshots;
source/native controls both hash to
`5029e44b9f632eef36e2f8eb2e0571f67cb432f6c03d835624063a1c39a13389`.
The expanded ASAN adapter fixture checks three complete lifetimes, fonts,
RGBA uploads, invalid dimensions, tint and shapes. External prebuilt raylib is
not sanitizer-instrumented; driver leak checks are disabled, as in #117.

Navigation, input/form/list/network widgets, NetworkManager/Wi-Fi and application
record/burn-in/profile behavior remain in progress. No production UI or device
acceptance is claimed by this intermediate stage.
