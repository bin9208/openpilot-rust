# Issue #17: selected-plan lateral uncertainty lookup

2026-10-23; parent task [#497](https://github.com/bin9208/openpilot-rust/issues/497),
defect [#17](https://github.com/bin9208/openpilot-rust/issues/17).
Base: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` on
`codex/fix-497-stale-runtime-defects`. Patch is uncommitted for parent review.
Hosted RED/GREEN evidence and device acceptance are pending.

## Owned changed paths

- `rust/crates/modeld/src/action.rs`
- `rust/crates/modeld/tests/action.rs`
- `rust/crates/drivingd/src/publication.rs`
- `rust/crates/drivingd/src/state.rs`
- `rust/crates/drivingd/tests/state.rs`
- `rust/crates/drivingd/tests/issue_17_49.rs` (shared with the #49 fix)
- `rust/tools/driving_daemon_reference.py`
- `docs/naver/issue-17-audit-fix.md`

## Mechanism and deliberately bounded correction

The original `modeld.py` reads `plan_stds[0, 10, Plan.POSITION, 1]`.
`parse_policy_outputs()` produces `(1, 33, 15)` and `Plan.POSITION` is
`slice(0, 3)`, so four-dimensional indexing raises; the catch substitutes zero.
The Rust helper previously discarded the selected prediction's standard deviations
and retained that fallback for parity.

The inferred intended contract is batch 0, trajectory sample 10, position-y
(flattened column 1). Rust's `DrivingPrediction.plan_std` already contains the
selected batch, so the corrected read is `[10][1]`. Position uncertainty is in
metres, rather than variance or log-standard-deviation: the parser has already
converted the model's normal output to standard deviations. Sample 10 is
`10 * (10/32)^2 = 0.9765625` seconds. The original name `y_std_1s` is approximate;
this fix does not introduce interpolation to exactly one second or select a new
horizon. Those would require a separate steering-policy decision.

The existing positive-base interpolation is retained: uncertainty <=0.15 adds
zero, >=0.25 adds twice the base, and values between interpolate linearly. Base
plus extra remains clamped to [0, 0.60] seconds; base <=0 disables smoothing.
Float32 uncertainty is promoted before interpolation, as the original `float()`
does. Nonfinite uncertainty (NaN or either infinity) retains the prior
zero-uncertainty, base-only fallback, including the existing [0, 0.60] cap.
No new steering threshold or tolerance is introduced. This inference identifies
the broken indexing; it does not establish
that the inherited thresholds, sample or smoothing policy are vehicle-approved.

Publication computes smoothing once from the actual selected prediction and uses
the same value for curvature filtering and next-frame lateral delay. Existing
custom/live-delay precedence, previous action Float32 feedback, model recurrent
state and source selection are preserved.

## P1 follow-up from independent review

Dewey identified a regression in the first #17 patch: a positive finite base
with NaN `plan_std[10][1]` made the corrected helper return NaN. Native parsing
does not reject that value. `complete_action()` then stored a NaN lateral delay;
the next frame consumed a NaN action time, producing NaN curvature and persistent
Float32 previous-action feedback. Publication validity still depended on the
existing calibration contract. The original failed lookup ignored uncertainty,
so this poisoning was introduced by the initial fix, not inherited behavior.

The follow-up rejects nonfinite uncertainty in the smoothing helper before
`from_plan()` or state feedback. It returns the same clamped finite base as the
old zero-uncertainty fallback. This does not sanitize plan measurements on the
wire, change calibration/freshness validity, reset recurrent state, or waive any
source-output comparison. The previously unsafe helper expectation that NaN
uncertainty should produce NaN has been replaced with fallback assertions.

Nonfinite **base-setting API policy is separate and unchanged** from the
pre-#17 helper: NaN base can still return NaN, positive infinity clamps to 0.60,
and negative infinity disables smoothing. This patch prevents newly introduced
uncertainty poisoning for finite settings; it does not add a new settings
validation policy or claim arbitrary nonfinite model/action inputs are safe.
Those inputs are outside this narrowly requested correction.

## Tests written before implementation

- `rust/crates/drivingd/tests/issue_17_49.rs` uses the original public API and real
  `Publication::build()`. It varies only `[10][1]` amid distractor uncertainties,
  checks deterministic serialized curvature, Float32 feedback, and following
  action delay for live and custom actuator delays. The old implementation fails
  when uncertainty first increases above the lower interpolation endpoint.
- `rust/crates/modeld/tests/action.rs` checks disabled/base/cap behavior, exact
  Float32-input interpolation examples and nonfinite-uncertainty fallback. A
  separate test records the unchanged nonfinite-base API policy. The existing
  action tests are retained.
- The P1 regression test was written before its guard implementation. It runs
  actual sequential `Publication::build()` calls for a NaN-uncertainty frame
  followed by a finite-uncertainty frame. Plan-derived curvature consumes the
  prior delay. It checks finite action times, delay and Float32 feedback, compares
  actions against a zero-uncertainty fallback control, and requires complete
  finite-frame recovery. The bad measurement remains NaN on the wire rather
  than being hidden; existing event-validity rules remain unchanged.
- `rust/tools/driving_daemon_reference.py` has one scoped #17 AST adaptation:
  replace the original impossible subscript with `[0, 10, 1]`, then insert a
  nonfinite-to-zero uncertainty fallback immediately before the exact original
  interpolation assignment. Both source expressions are guarded, so a changed
  lookup or interpolation requires review. All other original statements,
  output comparisons and existing numeric tolerances remain intact.

## Parent's hosted verification commands

Run only on GitHub-hosted Actions, with the repository's existing dependencies.
The standalone publication test can run against original production source for
actual RED without requiring the new helper signature. In a **dedicated baseline
job checkout** containing the candidate tests, restore these production files:

```sh
git restore --source=b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a -- \
  rust/crates/modeld/src/action.rs \
  rust/crates/drivingd/src/publication.rs rust/crates/drivingd/src/state.rs \
  rust/crates/drivingd/src/jetlink.rs rust/crates/drivingd/src/lib.rs \
  rust/crates/drivingd/src/daemon.rs
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-driving-modeld \
  --test issue_17_49 issue_17_selected_uncertainty_controls_curvature_and_next_delay -- --exact
```

Require a real assertion failure, not a compiler/setup failure. In a fresh candidate
job checkout run:

```sh
cargo test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-modeld -p openpilot-driving-modeld -p openpilot-jetlink --all-targets
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-driving-modeld \
  --features visionipc-ion --target aarch64-unknown-linux-gnu
```

For **P1-specific RED**, the original production source is insufficient: it
ignored uncertainty and therefore did not have this poisoning regression. In a
separate disposable GitHub-hosted candidate checkout, remove only the new guard
from the initial #17 implementation and retain the new sequential test:

```sh
python3 - <<'PY'
from pathlib import Path
path = Path('rust/crates/modeld/src/action.rs')
source = path.read_text()
guard = '    if !y_std.is_finite() {\n        return base.clamp(0.0, 0.60);\n    }\n'
assert source.count(guard) == 1
path.write_text(source.replace(guard, '', 1))
PY
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-driving-modeld \
  --test issue_17_49 issue_17_nan_uncertainty_does_not_poison_the_following_finite_frame -- --exact
```

Require assertion RED at the finite-state check. Run the same test on the
untouched candidate for GREEN, alongside the all-targets command above. These
follow-up hosted RED/GREEN results have not been observed by this worker.

Retain the existing original-model daemon comparison with its normal catalog,
model and IPC setup (also checks the #49 warning exactly once in the raw-enabled
scenario):

```sh
PYTHONPATH="$RUNNER_TEMP/model-msgq-python:$PYTHONPATH" python rust/tools/check_driving_daemon.py \
  --binary rust/target/debug/openpilot-driving-modeld --collector rust/target/debug/openpilot-logmessaged \
  --catalog "$RUNNER_TEMP/native-catalog" --models "$RUNNER_TEMP/original-models" \
  --output "$RUNNER_TEMP/driving-daemon"
```

No functional tests, builds, performance runs or fixtures were executed locally.
Local validation is rustfmt, Python AST syntax, and scoped diff review/check only.
Parent owns CI orchestration, exact-SHA Actions evidence, commit and integration.
Steering behavior and device/drive acceptance remain separate and unverified.

User guides are outside this worker's ownership. Parent must resolve paired-guide
requirements for the behavior change before integration.
