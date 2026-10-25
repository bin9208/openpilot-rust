# Independent diagnostic oracle captures

original.json pins six real unchanged diagnostic helper executions under
Xvfb/llvmpipe, including the three checker modes and arbitrary/empty mode input.
Every Pillow 12.3.0 / NumPy 2.5.3 i64 sample is hashed. trace-original.json
records the original raylib call order and arguments, shader strings, grid and
touch colors. It leaves the real raylib calls active; no candidate renderer is
imported. The raw boundary probe reproduced byte-identical PNGs for all four
distinct scenes; that probe is not the production Rust reference proof.

checker-original.json captures ten unchanged original checker runs with
independently injected child images/process errors, retaining real original
Pillow/NumPy decoding and subtraction. Content-addressed blobs/ contain only
tiny synthetic PNG or deliberately invalid image bytes. The captures cover
different pixels, incompatible shapes, source/native child failures,
missing/invalid files, grayscale/16-bit arrays and a raw Unix binary path.

The Rust reference owns the drawing/shader/overlay policy. raw-raylib.cc is a
separate owned resource/ABI subprocess using only the staged raylib library.
It never imports the candidate Renderer/Diagnostics or a Python interpreter.
Build it with build-ui-diagnostic-reference, then set
UI_DIAGNOSTIC_RAW_HELPER for ui-diagnostic-reference and the checker.
The original sources remain until package, real source/native and absence
gates pass. Detailed capture scripts/probes are ignored scratch evidence.

The native reference and checker were also executed with PATH=/nonexistent:
all six original mode PNGs matched byte for byte and all three real candidate
comparisons had zero pixel differences. These local proofs are distinct from
injected checker tests and are recorded in the ignored diagnostics evidence.

Source provenance binds the entire original.json SHA, every runtime/font pin,
namespace/font absence and five complete dependency records. The captured
host has no importable params_pyx binding, so multilang falls back to English:
LANG/LANGUAGE do not alter this scene. Newly importable Params bindings fail
closed for independent language/font recapture. BIG/SCALE/OFFSCREEN are forced
by the source helper, whereas ENABLE_VSYNC and BURN_IN presence are preserved.

The independent environment-original.json captures inherited numeric parse
errors, VSYNC, BURN_IN presence, forced scale, locale and RECORD policy. RECORD
launches the original FFmpeg argument list but this particular helper queues
no video frames. Missing FFmpeg fails before font setup; RECORD without burn-in
fails at the original missing-shader boundary; burn-in plus RECORD exports its
normal PNG and closes the owned recorder. Both owned processes are reaped on
failure. PROFILE_STARTUP retains its initialization-only, successful early exit
and creates no PNG. Its report gives actual native initialization elapsed time;
CPython cProfile's interpreter-specific per-function inventory is not emitted.

PROFILE_STARTUP scope: the source's cProfile context covers init_window (window,
optional recording process, font loading/patches and optional burn-in shader),
prints its top 25 Python functions sorted by cumulative time, then exits 0.
The native timer starts immediately before flags/window setup and ends after
normal font and optional shader initialization; it includes raw ABI round trips
and optional FFmpeg launch, but excludes provenance checks and raw-helper spawn.
It reports that elapsed duration and the same startup heading/readiness line.
It does not claim Python per-function call counts/timings or a speed comparison.
Both paths stop before drawing/export, leave an existing requested PNG untouched,
and close window/font/target/shader and any recording subprocess. Source
initialization failures remain failures even with profiling requested. This
helper never enqueues a recording frame in either implementation.
