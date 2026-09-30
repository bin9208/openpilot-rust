# Model output interpretation and publication

Issue [#16](https://github.com/bin9208/openpilot-rust/issues/16) ports the output
semantics used by modeld and dmonitoringmodeld into `openpilot-modeld`. The full
runtime gate in [#1](https://github.com/bin9208/openpilot-rust/issues/1) remains
open. This increment does not select a production daemon or authorize a device
connection, installation or test.

## Source and contract

The source is the existing `parse_model_outputs.py`, `fill_model_msg.py`, action
functions in `modeld.py`/`controls/lib/drive_helpers.py`, and driver output
functions in `dmonitoringmodeld.py`. Input slices come from the compiled artifact
metadata; parsing rejects missing, out-of-range and incorrectly sized outputs.
Driving plans, uncertainty, lanes, edges, leads, desires, meta probabilities,
pose transforms and both driver predictions become fixed-size Rust values.

The new code builds the full original `modelV2`, `drivingModelData`,
`cameraOdometry` and `driverStateV2` payloads through the generated cereal schema.
It preserves frame IDs, saturated frame age, EOF/publish timestamps, frame-drop
percentage, execution time, raw prediction bytes and message validity. The
compact driving message derives its action, lane metadata and polynomial path
from the model message, after the caller supplies lane-change metadata.
The full daemon's desire helper, calibration, camera pairing, runtime timing,
Jetlink updates and actual message-bus loop remain separate integration work.

FCW keeps the original two- and five-frame histories and strict probability
thresholds. Confidence updates its 25-element float32 buffer only at frame IDs
divisible by 40 and applies the original diagonal score and class thresholds.
Line times preserve the source's interpolation, finite fallback and monotonic
correction. Position uses the original float64 degree-four least-squares map,
precomputed from `np.polynomial.polynomial.polyfit(T_IDXS, np.eye(33), 4)`.
NaN columns and infinite right-hand sides retain the observed original LAPACK
behavior; no invalid output is silently replaced with a plausible path.

Action calculations retain stopping tests, interpolated acceleration, direct
action overrides, maximum planned velocity, low-speed curvature hold and
smoothing. The source's direct-action division is float32 under NumPy 2, before
float64 smoothing. Returned cereal action fields round to float32 before feeding
the next frame. The existing invalid four-dimensional `plan_stds` lookup falls
back to zero uncertainty; this port preserves that behavior rather than changing
the lateral smoothing policy during a language migration.
The pre-existing lookup defect is tracked separately in
[#17](https://github.com/bin9208/openpilot-rust/issues/17).

## Numerical architecture boundary

The reference environment pins NumPy 2.4.6 and pycapnp 2.1.0, matching CI. NumPy's
[float32 exponential dispatch](https://github.com/numpy/numpy/blob/b832a09cf2a169c833dd2371e7c07aa00b293242/numpy/_core/src/umath/loops_exponent_log.dispatch.c.src#L1275-L1308)
uses a rational polynomial on AVX2/FMA and AVX512 x86, and scalar `expf` on ARM.
These paths can differ at a lead hypothesis tie: the x86 softmax of `[3e-8, 0]`
rounds both probabilities to 0.5. Using a different exponential changes the
selected hypothesis even though the probability error is small.

`numpy_exp.rs` therefore preserves the x86 polynomial's fused operations and
float32 constants when those CPU features are present. Other targets retain
scalar `expf`. The NumPy copyright and BSD license are retained in
`rust/crates/modeld/NUMPY-LICENSE.txt`. This is a source-based architecture
boundary, not a tolerance added to lead selection. The exact device NumPy/libc
build and target output agreement still need the eventual full-runtime device
comparison; host results do not establish ARM execution equivalence.

## Repeatable evidence

Before implementation, missing parser/publication/driver APIs failed their new
tests. The implemented crate passes 22 Rust tests. An independent action oracle
ran the actual source function bodies over 2,500 cases, including direct action,
stop thresholds, speed boundaries, delays and smoothing; the largest float64
difference was 3.56e-15. It exposed and locked the float32 direct-division case.
Independent review also reproduced a non-finite interpolation mismatch. Three
new Rust regressions failed before the repair and passed afterwards: exact
samples bypass a non-finite predecessor, interpolation retries from its other
endpoint, and equal infinite endpoints retain their value. The same helper
governs curvature, acceleration and the one-second stopping preview.

`check_model_outputs.py` executes the Rust examples and decodes their actual
cereal bytes with the complete original Python schema. It compares all present
fields, including raw bytes and message validity, against the original parser
and publication functions. The only substituted source dependency is message
allocation for driver monitoring; all output mathematics and field mappings are
the actual source functions. The action reference returns real cereal Actions.

The synthetic corpus has 240 driving frames, 240 mixture-lead frames and 16
driver frames. It covers history turnover, confidence updates, both lead formats,
ties and near-ties, degenerate/non-monotonic plans, NaN/infinity, raw-output
enablement, frame age, lane-change metadata and dropped-frame pose validity.
The captured corpus uses actual native pipeline outputs, already compared with
the original compiled model execution. Required CI repeats it for both camera
resolutions and 128 driving/3 driver frames per resolution.
The corpus also includes 15 independent action interpolation edge frames so a
non-finite action cannot mask subsequent cases through the previous-action state.
Local release-mode validation compared 956,558 fields across the synthetic
corpus plus six captured driving and three captured driver frames. All compared
fields passed their predeclared bounds, and the exponential probe passed exact
bit comparison for all 250,008 values (with equivalent NaNs).

The comparison bounds were fixed before implementation: probability/standard
deviation and other float32 fields use 1e-6 absolute/relative; polynomial
coefficients use 2e-5; the separate float64 action oracle uses 1e-12. Discrete
choices, enums, IDs, validity, FCW and confidence classes must agree exactly.
The exponential probe compares 250,008 values bit-for-bit, with equivalent NaNs.
No threshold or validity policy was relaxed to pass a comparison.

Exact-head CI, independent review and post-merge results are recorded in #16.
These are host output-semantic checks, not GPU execution, loaded driving or
measured CPU savings. Full runtime startup, logging, uploads and all remaining
project-owned services are still required before the first device handoff.
