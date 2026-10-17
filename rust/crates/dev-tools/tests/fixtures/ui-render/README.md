# Independent shared-widget render captures

The unchanged original `check_ui_render.py` selects six ordered scenes and 112
render frames. `scenes.json` preserves those inputs. `original.json` records
original PNG hashes, full state and unnormalized Pillow/NumPy sample evidence.
The checker fixture contains 13 independent process/array/state cases, including
failure ordering and exact Python state equality. No Forms float tolerance is
applied to render comparison.

The native reference implements Label, UnifiedLabel, five Button styles and
Toggle through an owned raw raylib ABI; it does not import candidate widgets or
use fixture images as drawing policy. The ordinary corpus contains six default
and 24 generic cases covering all eight font selectors, spacing, wrapping,
scrolling, shimmer, scale, alignment, enabled state, input and invalid selectors.

The lossless raw corpus contains 70 unchanged-original captures: 38 admission
and constructor cases, 13 event conversion/indexing cases, four Python whitespace
controls, five embedded-NUL/emoji cases and ten repeated-click controls. It
preserves raw Toggle values and surrogates until the source consumes them. Event coordinates are converted only
when the enabled primary collision branch consumes them. Embedded NUL retains
raylib's C-string prefix independently for each emoji-delimited text segment.

The 14 environment cases compare real CLI exits, PNG/state artifacts, profiler
termination, debug overlays, VSYNC, burn-in and recording. Recorder evidence
checks inherited FFmpeg arguments, bottom-up RGBA bytes/hashes, every-third-frame
submission, stdin EOF and owned-process cleanup. Startup profiling preserves its
early successful exit before widget construction or PNG/state creation. Native
timing covers window, target, recorder and font initialization, and native render
work, rather than reproducing Python cProfile call stacks or import timings.

`provenance.json` pins 37 actual imported policy/source/font files, dependencies,
font absence and Params import layout. Its literal manifest digest protects the
required key set. Only the two original checker/helper paths may be absent under
immutable original hashes; modified reintroductions are rejected. Other live
pins remain mandatory. Copied-root tests exercise both originals absent together.
Raw malformed replies, EOF and command errors exercise owned-child reaping.

Raw captures, scene logs, capture scripts and generated PNGs remain in ignored
`.analysis/scratch/2026-10-18-ui-render/`. The complete 30 ordinary / 70 raw /
14 environment production gate and strict Clippy passed after the repeated-click
fix. Its ten cases preserve every successful release, including conversion of an
arbitrary initial value to boolean on the first click. The six-scene native-only
CLI proof passed before the final retirement; the parent separately records the
fresh CLI and source-absence proof after deleting both originals.

Shared environment and emoji raster helpers retain the provenance and Pillow
license recorded in `../ui-forms/README.md` and `../ui-forms/PILLOW_LICENSE`.

After both Python tools were removed, all seven checker/reference tests passed
again (178.44s reference gate), including live drawing and environment checks.
The native-only six-scene/112-frame CLI matched every pixel and full state with
PATH=/nonexistent. Scoped strict Clippy passed against the final registered source.
