# Model renderer source and numerical contract (#148)

## Scope and reference

Baseline `73960ff11b8ce1c6a110bc237f65e1e323094f74`. The original
`selfdrive/ui/onroad/model_renderer.py`, `mici/onroad/model_renderer.py`,
`onroad/path_geometry.py`, and `ui/road_markings.py` are the implementation and
visual reference. Shared viewport/font/native-graphics rules are in
`startup-ui/DESIGN.md`; the parent application's product contract preserves
original UI behavior. This slice owns both native renderer Widgets, their active
render call graphs and geometry, source text/shape composition, and render timing.
The big display's commented legacy draw pipeline stays disabled. This is rendering
of already-produced model/radar data, not detection or vehicle-control policy.

## API and ownership

Each display exposes `ModelRenderer::new(Context)` and the framework `Widget`
contract. `set_transform([[f64; 3]; 3])` accepts the camera transform and narrows to
Float32 at the same point as source. Typed cereal readers consume existing
SubMaster messages and preserve receive-frame, updated, valid and alive gates.
Context owns Params, UiState and subscriptions; the parent owns camera/HUD/alerts,
application composition and display placement. Renderer modules remain separated
by geometry, path modes, lane markings, lead/radar overlays, and drawing primitives.
Minimal crate exports, default-font measurement, rounded-outline thickness and the
shared render-diagnostics helper are coordinated with the parent UI worker.

## Numeric acceptance fixed before implementation

Preserve source Float32 arrays, Float64 scalar/interpolation paths, explicit
narrowing and operation order. Compare complete projected vertices, retained state,
colors, draw commands and ordered Params reads with unchanged source on identical
inputs. The acceptance target is exact Float32 values and discrete decisions;
Float64 state/intermediates use absolute plus relative 1e-11. A numerical budget
never permits a different clipping mask, topology, color, draw order, label,
visibility gate or state transition. Do not widen a budget after a failing case.

NumPy 2.5.3's 3x3 matrix products and its single-point products can use different
accumulation orders; the implementation must verify each source dtype/kernel path.
Blindspot projection explicitly uses nonfused three-term Float32 products. Path
sampling preserves the two interpolation stages and repeated-x behavior. Depth
acceptance remains abs(z)>=1e-6; the clip margin remains 500 logical pixels;
inversion filtering uses the source running minimum and inclusive comparisons.
External NumPy is oracle-only; the native renderer never executes Python.

## Visual tokens and composition

Reuse original licensed fonts and the actual native raylib/GL primitives. Preserve
source primitive calls and their sequence, including solid versus shaded ribbons,
shader gradient stops, outline widths, default-font radar-box measurement and
Display-font labels with eight-direction outlines/shadows. No reference image may
substitute for live geometry or text. Source palettes are the ten Carrot RGBA path
colors, throttle/no-throttle three-stop gradients, yellow/white classified lanes,
road-standard-deviation red/blue ramp, blindspot yellow/green, lead-source colors,
and tire danger/pulse ramp; encode these as reusable constants/helpers exactly.
Big lead rectangles retain roundness0.15/segments12/stroke3; compact radar boxes
retain roundness0.28/segments8. Source typography and fixed geometry are inherited,
including Korean E2E text and compact text placement, without visual redesign.

## Temporal and state contract

Big rendering refreshes its eight Carrot Params at one-second source intervals and
rebuilds active projected geometry each draw. Preserve all path modes, speed-driven
animation wrap, lane-plan selection, follow-distance markers, lead smoothing,
color selection, brake outlines and tire pulse phases. Compact rendering refreshes
geometry only for model/radar updates or transform dirtiness, retains its exact
filter/cached-lead behavior, and applies source status/throttle/experimental gates.
Neither display may weaken stale-message or validity checks. Runtime diagnostics
retain per-stage elapsed/thread-CPU accumulation and native runtimeTiming emission.

## Verification matrix

Run complete source/native traces through startup, repeated frames, Params refresh,
transform changes, valid/stale/missing data, empty/short paths, repeated/decreasing
x, near-zero/negative/nonfinite depth and clipping boundaries. Exercise every path
mode0..15, all palette/automatic/brake decisions, straight/curved/hill paths,
solid/dashed/double/unclassified lanes, lane probabilities around0.3, road edges,
blindspots and lane-change assist, both leads and source types, cut-in/future radar
markers, metric/imperial labels, follow distance, hold/traffic states, tire danger
and pulse phases, compact torque colors and throttle/experimental transitions.

Render matching original/native scenes with real pinned raylib under a dedicated
owned Xvfb display. Capture each scenario/state and both display geometries; compare
pixels and independently inspect captures, including CJK and intermediate animation
frames. Native geometry/state parity alone is insufficient visual evidence.
New native calls require adapter sanitizer coverage; safe geometry/state tests get
Miri where supported. Record every invocation, observable, artifact and binary hash.

## Constraints and remaining gates

Use isolated synthetic Params and rust-probe IPC namespaces, never production
subscriptions, vehicles/C3X, real CAN, NAS or accounts. Check disk before each build,
install or large copy: preserve25GiB plus growth; use coordinated cache75 with
incremental disabled and bounded jobs2. Do not mutate UIworker cache84. Keep #150
worktree and retained proof binaries intact. This slice is intermediate #148 work;
parent composition, complete runtime normal startup/log upload, AGNOS packaging,
and first user-performed device comparison remain separate gates.
