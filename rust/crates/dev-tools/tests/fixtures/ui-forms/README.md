# Independent Forms captures

The unchanged original checker has18 fixed scenes and388 input/render frames.
`scenes.json` preserves ordered policy inputs. `original.json` records actual
source PNG hashes, unnormalized Pillow/NumPy i64 sample hashes and full state
history hashes. The `*-state.json` histories are synthetic. Raw PNGs, logs and
capture scripts remain in ignored scratch.

`checker-original.json` contains35 independent original executions with injected
child images, states and process errors. Real Pillow/NumPy arithmetic and the
original recursive state comparator remain active. Cases cover selection/order,
partial files, process failures, invalid images/state, grayscale/16-bit arrays,
raw Unix argv, numeric tolerance, bool/large integer/NaN/Inf behavior, Unicode
and surrogate assertion strings, deep comparison and deep mismatched-value repr.
`repr-nonprintable.json` is captured from CPython3.12 Unicode15 isprintable.

The independent reference covers all8 widget families through an owned raw
raylib ABI, without importing candidate UI widgets or stored rasters as policy.
The current green image/state corpus has69 cases:18 defaults,12 InputBox,
9 HTML,7 scale/language,2 emoji regressions,15 generic interactions and6 debug
scenes. Input drawing traces use real original measurements to compare every
frame and draw event. Additional coverage includes65 helper admission cases,
23 environment/error CLI cases,14 direct emoji bitmap hashes, source/font/asset/
dependency/namespace drift, ABI failure stopping and owned-child reaping.
The admission suite now contains65 original cases, including cursor neighbors,
huge integer limits, ignored surrogate/deep fields, string scales and lazy float
touch-slot failures. Both original helper/checker files were retired after the
native-only18-scene proof passed again with those files absent. Only those paths may
be absent under immutable captured hashes; all runtime pins remain required,
and modified reintroduced originals fail the guard.

Startup profiling preserves success exit before widget construction, PNG and
JSON. Native timing covers window/target/recorder/font initialization; it emits
an explicitly native entry in place of Python cProfile tables. Render profiling
preserves completed-frame termination before export and reports native timing.
Recorder tests compare inherited FFmpeg argv, every-third completed bottom-up
RGBA frame, exact pixel hash, stdin EOF and child cleanup.

Emoji raster policy provenance is the source-pinned Pillow12.3.0 sdist SHA256
3b8182a766685eaa002637e28b4ec8d6b18819a0c71f579bf0dbaa5830297cce:
`src/_imagingft.c`, `src/PIL/ImageDraw.py` and `src/libImaging/Paste.c`.
Copyright and license are retained in `PILLOW_LICENSE`. The renderer uses pinned
rustybuzz0.20.1 to read font shaping/bitmap tables, independently reproducing
FreeType premultiplication, Pillow unpremultiplication/mask paste, positioning
and clipping. Noto font bytes and source widget policies are manifest-pinned.

The subsequent codepoint fixtures (`codepoints-original.json` and
`codepoints-neighbors-original.json`) contain 21 additional unchanged-source
cases. They distinguish lazy UTF-8 failures from successful zero-frame,
truncation, deletion and masked-password paths, including raw surrogate text in
state snapshots and high/low surrogates assembled by separate key actions. The
new CLI regression reads these fixtures losslessly. These21cases passed with
65admission cases,69earlier PNG/state scenes,strictClippy and the current
18-scene native-only checker; see codepoints-package/clippy/native-only logs.

The family text integration suite contains208 independent original CLI cases
across five tests:

- 69 codepoint/property/selection/NUL/Mici membership cases.
- 47 arbitrary-value, mutable Mici list alias, character iterable and lazy Slider
  representation-depth cases.
- 65 Input/Keyboard lists, Confirm setter timing, outer touch slots and Slider
  shimmer measurement cases.
- 24 raw List visibility values (including lone surrogates), lazy float cursor
  keys, Keyboard list-key neighbors and Options touch-slot cases.
- 3 JSON encoder boundary cases: depth993 preserves the exact complete state
  byte hash; depths994/995 fail after PNG export without creating state JSON.

The helper ran under CPython3.12.15 before candidate integration. Property,
selection, character, admission and tail provenance files record original
source and fixture hashes. Expected PNG hashes, full lossless state values and
failure artifact boundaries come from that independent source execution.
Raw captures and scripts remain in ignored scratch. Early manual failure
probes missing the raylib library environment were invalid and are not evidence;
the compiled fixture gates and correctly configured native-only proofs provide
candidate validation.

Issue #350 repeated-release regressions add 12 unchanged-original cases in
repeated-release-original.json and repeated-release-neighbors-original.json.
They preserve every List toggle/description update, Keyboard character/Enter,
and Confirm/Cancel pop across two/three releases and mixed buttons. The repaired
reference and current candidate matched all twelve PNGs/states; all twenty
affected Forms integration tests, strict Clippy, and the 18-scene/388-frame
native-only checker passed on an owned Xvfb display.

Admission PNG assertions remain byte-exact. If UI_FORMS_FAILURE_OUTPUT is set,
a mismatch retains only that case's PNG/state, original scene/case, child logs,
source provenance and render metadata (driver environment plus helper/library
hashes). This supplies evidence for platform-specific failures without saving
additional successful PNGs or changing the expected hash.

Repeated-release captures (2026-10-18) preserve12 unchanged original scenes,
including double/triple keyboard input, list toggles/descriptions and Confirm/
cancel/mixed results. Before repair10 reference scenes and5 production Confirm
scenes differed. All12 now match original PNGs and states; the18scene/388frame
gate also passes. Exact source histories/hashes remain in
`repeated-release-original.json` and `repeated-release-neighbors-original.json`.

`confirm-original-draw.json` preserves the original drawing calls for the
`irrelevant-input-prop` admission scene. The unchanged helper was recovered only
for capture from commit `3669ec8c6f391275316db31bfd715ab23f0da5da`; its SHA-256 is
`2220f3192136ead8d283cf8276ca2120a85198978bbca79c0f1286b8690dc365`.
The trace contains 21 mechanical calls, 11 measured font extents, exact scene and
full state, six font byte hashes, and the original PNG hash. Font point ranges
losslessly encode the observed argument lists; counts and source provenance are
bound to the immutable trace digest in the test helper.

CI run `39082659846`, job `117542952197`, independently executed the original and
native helpers on one display. Both produced the same `48ced25f…` image and
identical state, with zero differing samples. The local original instead
produced `35a7f1f0…`. This exposed a renderer-dependent fixture assumption.
The admission test now renders the immutable original calls using the same
mechanical helper, library and environment as the native reference, then keeps
an exact PNG byte comparison and the original full-state comparison. All eleven
font measurements must retain their exact binary32 values. Original hashes
remain provenance; no alternate-hash list, pixel tolerance, interpreter fallback
or captured-scene lookup was added to production code.

Local mechanical replay reproduced the corresponding unchanged-original image
under llvmpipe's normal and SIMD-disabled paths and under softpipe. The test
rejects altered capture/source/scene/state bindings. A rendered-image regression
changes exactly one RGB sample and requires rejection. The temporary CI original
probe remains diagnostic only until the next remote run verifies this repair.

The follow-up `admission-original-draw.json` extends this same independent oracle
to **all 44 successful PNG cases** in both admission fixtures. The other 21 cases
retain their original success/error, state-file and PNG-existence assertions.
The corpus includes Input cursor, password and touch paths, Confirm variants,
HTML, and zero/negative-frame exports. It binds the original helper and input
provenance hashes, complete original state, environment, semantic scene or exact
raw scene bytes, original PNG digest, six original font byte hashes and lossless
font point ranges. Raw JSON strings are retained without parsing or reducing
their original depth/surrogate coverage. No candidate drawing policy generated
the expected calls.

CI job `117550472152` exposed the next fixed-image assumption:
`cursor-bool` differed in exactly 206 pixels, a one-column horizontal shift of
the 103-pixel cursor line. Both captured states were unchanged. The original
Input blink advances once per rendered frame, independent of wall-clock time.
Immediate and deliberately delayed original runs preserve the same calls and
state. The corpus replay compares exact native PNG bytes against original calls
rendered by the same current mechanical helper/library/environment; all original
font extents still require bit-exact binary32 agreement. Each successful raster
must have a bound capture, and altered inputs, state, environment, commands,
measurements, source or provenance are rejected. Failure evidence retains the
complete original capture and current-context expected PNG alongside the actual
PNG. Cross-runner verification of this extension remains pending.

The expanded local admission suite passed all six tests (201.97 seconds),
including every original admission case and all 44 exact raster replays. The
SIMD-disabled full suite also passed all six tests (427.41 seconds), followed by
an independent parent rerun (199.82 seconds). Remote CI remains the next
verification gate.
