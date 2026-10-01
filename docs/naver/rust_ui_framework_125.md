# Shared Rust UI framework (#125)

Tracks [#125](https://github.com/bin9208/openpilot-rust/issues/125) under the
[full runtime port](https://github.com/bin9208/openpilot-rust/issues/1).
Source: `openpilot/system/ui/lib` and `openpilot/system/ui/widgets`, inherited
from `8a3a462d`; original MIT licensing and assets remain intact.

## Implementation coverage

`openpilot-ui-framework` owns the shared application loop, widget state and
rendering policies. It does not invoke Python. The unchanged Python modules
are executed only by host comparison tools.

| Source responsibility | Native implementation |
| --- | --- |
| Application window, navigation stack, tick callbacks, input and shutdown | `application`, `stack`, `widget`; startup-ui renderer and input |
| Visibility, enable/touch predicates, two touch slots, parent clipping, delayed clicks | `widget` |
| HBox, both scroll panels, navigation transitions, Mici/TICI scrollers | `layouts`, `scroll`, `navigation`, `scroller`, `scroller_tici` |
| Text layout/measurement/drawing, labels, outlines, shadows, emoji | `text_layout`, `label`, `unified_label`, `styled_text`, `emoji` |
| PO language catalogs, plural selection and Params language setting | `multilang` |
| Buttons, toggles, icons, HTML, input, keyboards, dialogs, lists and slider | corresponding public widget modules |
| Wi-Fi list, password, forgetting, APN and tethering screens | `network`, typed `openpilot-wifi` session from #135 |
| Polygon triangulation, solid fill and gradient shader | `polygon`, startup-ui `renderer_polygon` |
| EGL NV12 image lifetime, initialization/retry and GLES texture binding | startup-ui `egl` and narrow C++ ABI adapter |
| Burn-in mitigation, touches/grid/FPS, startup/render profiling, recording | startup-ui `diagnostics`, `fps`, `recording` |

The application renders before dispatching input, preserves duplicate tick
suppression and navigation callback ordering, and owns recording-child shutdown.
Raylib logs pass through the existing Rust logging producer and IPC path.
Both standalone startup surfaces use the same diagnostics implementation.
Coordinates accept zero/negative values while allocated dimensions remain
positive; this matters for the Wi-Fi scissor rectangle at the screen origin.

Color emoji uses locked Rust `rustybuzz`/`ttf-parser` and `png` with the source
NotoColorEmoji bitmap strike, baseline and alpha arithmetic. Wi-Fi and
NetworkManager transport policy is documented in
[Wi-Fi validation](../rust-port/wifi-validation.md).

## Reproducible host gates

Reserve at least 25 GiB plus estimated build growth before every build;
use an inactive coordinated target directory and disable incremental builds.
`STARTUP_UI_RAYLIB_ROOT` is the staged, lockfile-pinned comma-deps-raylib native
install; `STARTUP_UI_RAYLIB_LIBRARY` names its native plugin. The Python oracle
environment needs the source dependencies, numpy, Pillow, python-xlib and pyzmq;
recording checks require ffmpeg/ffprobe. An existing private Xvfb display is used.

```sh
CARGO_INCREMENTAL=0 cargo build --manifest-path rust/Cargo.toml \
  -p openpilot-ui-framework -p openpilot-startup-ui --examples --bins --locked -j2
cargo test --manifest-path rust/Cargo.toml \
  -p openpilot-ui-framework -p openpilot-startup-ui --locked -j2
cargo clippy --manifest-path rust/Cargo.toml \
  -p openpilot-ui-framework -p openpilot-startup-ui --all-targets --locked -j2 -- -D warnings
python rust/tools/check_ui_framework.py \
  --target "$CARGO_TARGET_DIR/debug" --output "$EVIDENCE/framework" --display "$DISPLAY"
python rust/tools/check_ui_emoji.py \
  --binary "$CARGO_TARGET_DIR/debug/examples/emoji_raster" --output "$EVIDENCE/emoji"
python rust/tools/check_ui_translations.py \
  --binary "$CARGO_TARGET_DIR/debug/examples/translation_trace" --output "$EVIDENCE/translations"
python rust/tools/check_startup_ui.py --target "$CARGO_TARGET_DIR/debug" \
  --output "$EVIDENCE/startup" --display "$DISPLAY" \
  --raylib-root "$STARTUP_UI_RAYLIB_ROOT" --raylib-library "$STARTUP_UI_RAYLIB_LIBRARY"
```

The framework gate records exact invocations, source/native state and captures.
It covers widget and scroll traces, navigation and stack lifecycle, rendered
controls/Korean/emoji, forms and keyboards, network commands, polygons,
diagnostics, actual XTest typing/click closure, pause/profile/ticks, ffprobe
recording results, private logging IPC, FPS decisions and seven EGL ABI cases.
The separate startup gate checks original startup screenshots/state, live input,
owned child lifecycle and the AddressSanitizer native adapter.

Pixel comparisons require zero differing pixels; discrete state must match.
Existing scroll numeric bounds (1e-5 offset, 1e-8 velocity) and forms 2e-5
absolute arithmetic bounds remain unchanged. The source empty-gradient polygon
raises `AttributeError` through `pyray.WHITE`; the Rust caller receives an error
for that same unsupported input rather than silently inventing a fill.

## Evidence and limits

Final attempt artifacts are under `.omo/evidence/ui-framework-125/resume2/`;
`receipt.json` records scenarios, invocations, observables, hashes and paths.
These private local artifacts are not committed. The parent performs a separate
visual/source audit before integration.

External dependencies remain explicit: raylib, font data, GL/X11 or board
EGL/GLES/DRM/GBM, NetworkManager/D-Bus, Linux input and ffmpeg. The EGL gate uses
a shared private C ABI fixture; it is not a real DMA-BUF device test. The
prebuilt external raylib/graphics driver is not ASAN-instrumented and driver leak
checks are disabled. ARM/AGNOS execution, product `selfdrive/ui`, production
selection and complete normal startup/upload are later integration gates.
No C3X/vehicle/NAS access, device acceptance or CPU savings are claimed.

Docs-Not-Needed: native shared-library conversion preserves existing settings
behavior and does not change production selection or public user instructions.
