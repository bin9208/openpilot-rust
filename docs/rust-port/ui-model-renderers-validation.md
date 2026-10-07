# Native onroad model renderers (#148)

This component implements the active call graphs of both original ModelRenderer
Widgets from baseline `73960ff11b8ce1c6a110bc237f65e1e323094f74`, plus
`onroad/path_geometry.py`, `ui/road_markings.py` and the big renderer's
`render_diagnostics.py`. The source files and existing assets retain their original
licensing and provenance. The implementation contract is
[`DESIGN.md`](../../rust/crates/ui-application/src/onroad/model_renderer/DESIGN.md).

Both display modules expose `ModelRenderer::new(Context)` and
`set_transform([[f64; 3]; 3])`; the transform narrows to Float32 where the source
narrows it. They consume the parent's typed SubMaster and UiState. The parent
continues to own camera/VisionIPC, HUD, alerts, display composition and scheduling.
The original big renderer's commented legacy pipeline remains disabled. This code
renders existing model/radar results and does not change detection or lead policy.

The geometry, palette, path animation, lane markings, blindspots, lead/radar labels,
follow markers, hold/traffic labels, tire pulse and compact throttle/torque filters
preserve their source operations. Shared graphics changes add actual raylib
`MeasureText` and `DrawRectangleRoundedLinesEx` calls, retaining original default
font metrics and stroke width. No captured image substitutes for live rendering.
The pinned native raylib/GL library and original font assets remain external
runtime dependencies; NumPy, pycapnp, pyray and Python are validation-only.

## Reproduction

Use Rust 1.94.0, the repository's pinned source-oracle Python environment
(NumPy 2.5.3 and comma-deps-raylib 6.0.0.1.post103), and the matching native raylib
headers/library. Set `STARTUP_UI_RAYLIB_ROOT` for builds and `LD_LIBRARY_PATH` for
execution. Run on a dedicated Xvfb display using `DISPLAY`; these are host GL
checks, not device timing measurements. Before builds reserve 25 GiB plus the estimated growth.
Use a coordinated target cache, `CARGO_INCREMENTAL=0`, and `-j2`.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-ui-application \
  --example model_geometry --example model_render -j2
PYTHONPATH=. python rust/tools/check_ui_model_geometry.py \
  /owned/target/debug/examples/model_geometry /owned/evidence/geometry
PYTHONPATH=. python rust/tools/check_ui_model_render.py \
  --binary /owned/target/debug/examples/model_render --evidence /owned/evidence/render
python rust/tools/check_ui_model_adapter.py --target /owned/target/debug \
  --raylib /owned/raylib --evidence /owned/evidence/adapter
cargo test --manifest-path rust/Cargo.toml -p openpilot-ui-application --lib model_renderer -j2
cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml \
  -p openpilot-ui-application --lib model_renderer -j2
```

The render checker needs `strace`. It creates an owned `rust-probe-*` namespace,
uses serialized synthetic cereal Events and private Params roots, captures real
GL output on every frame, and cleans its namespace. The unchanged original source
classes execute with controlled IPC, clock, Params and UI-state inputs. Native
Params read order is recovered from actual file-open syscalls scoped around the
renderer; UI fixture construction is excluded. Original shader, text and shape
calls still execute while their arguments are recorded.

Geometry checks compare all Float32 outputs exactly and Float64 samples at
absolute/relative 1e-11. Full-render checks require identical topology, colors,
ordered commands, discrete decisions and Params reads. Float32 command/geometry
values are exact; retained Float64 filter state uses the same 1e-11 bound. All
captured pixels must be identical. The six overflow cases exercise nonfinite
Float32 projection via finite Float64 transforms, preserving the source's
narrowing and rejected masks; their NumPy warnings are expected.

The adapter checker uses ASan and UBSan with halt-on-error: eight create/destroy
cycles exercise 1,024 Unicode/default-font measurements and 2,048 rounded outlines
in actual GL. Driver leak checking is disabled; this does not claim driver leak
coverage. Miri covers the safe refresh boundary, projection rejection and repeated
stage-timing accumulation. An ARM build and actual ARM source/native geometry
comparison under QEMU provide architecture evidence; full ARM GL/device behavior
remains outside these host checks.

## Recorded component result

The final host run passed 32 full-size scenes / 458 frames and 16 compact scenes /
266 frames: 195,990 ordered draw commands and 352 actual Params reads match the
original, and all 724 source/native frame pairs have zero differing RGBA values.
The 324-case geometry suite passed on both host and actual ARM Python/native
execution. Both ARM examples built successfully. Three focused unit tests and the
same safe-state scenarios under Miri passed, as did the ASan/UBSan adapter cycles,
Clippy with warnings denied, formatting and Ruff.

Manual visual inspection covers the 48-scene contact sheets and full-size CJK,
lane, blindspot and path captures. An initial synthetic transform reversed the
horizontal camera axis and culled filled polygons; those preliminary captures are
retained as harness evidence, not used as the final visual gate. The final suite
uses the original positive device-y/view-x convention and visibly renders the
fills and shader gradients. Source radar-label overlap in crowded synthetic scenes
is preserved; this component does not redesign layout.

## Delivery limits

The component receipt records invocations, observed results, retained executable
hashes, source hashes and capture paths. Independent component review and parent UI
integration remain separate gates. This is not complete project runtime delivery,
normal-startup/log-upload acceptance, measured CPU savings, or approval for a C3X
trial. No vehicle, CAN interface, NAS or user account is accessed. First device
comparison remains the user's step after the complete runtime candidate is ready.
