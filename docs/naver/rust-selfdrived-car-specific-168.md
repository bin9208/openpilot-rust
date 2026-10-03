# CarSpecificEvents native policy (#168 under #1)

This isolated host slice implements the unchanged
`openpilot/selfdrive/car/car_specific.py::CarSpecificEvents` policy. It is an
intermediate runtime conversion result; daemon startup, complete-runtime
inventory, log upload and the first user device comparison remain #1 gates.
MockCarState GPS socket selection is separate and is not included in this slice.
No device, C3X, real CAN, NAS or service was accessed. No device timing,
performance or CPU-saving claim follows from these checks.

## Native boundary and source parity

`rust/crates/selfdrived/src/car_specific.rs` owns all mutable counters, flags,
the eight-sample Hyundai deque, the 100-frame settings refresh and the Tesla
rising-edge latch. The `car_specific/` directory separates complete cereal field
extraction, common event decisions and exhaustive brand dispatch. Brand names
outside the source's named cases retain its common-policy path. Defined gear,
button and network variants use full-schema cereal enums; out-of-schema values
fail explicitly. Wire structure errors, unknown enum ordinals and invalid UTF-8
brand text retain distinct typed errors.

`CarSpecificEvents::new(CarParams)` and
`update(&CarInputs, &mut impl CarSpecificParams, &Catalog)` return sorted
`Vec<EventName>` with duplicates. The caller supplies the existing catalog; the
policy does not rebuild it per frame. `CarParams::read`, `CarState::read` and
`CarControl::read` extract every field consumed by the original policy.
`create_common_events` and `update_params` retain independently callable source
behavior. The source accepts `extra_gears` but never reads it; this port retains
that fact instead of adding a new gear eligibility rule.

The Params trait makes ordered synchronous reads/writes observable. Its
`NativeParams` implementation reuses the existing source-compatible boolean
reader and physical native storage. Cython ignores the return status from
`putBool`; native I/O write failures are likewise ignored, with the original
shutdown/Tesla latches advancing. Unknown-key errors remain errors. The checker
uses the unchanged original compiled Cython binding on a separate physical Params
directory, including directories placed at key paths to force read/write failures.

The oracle compiles the unchanged class AST and executes actual imported
opendbc constants. It reuses the unchanged source Events class and source event
catalog. It records full mutable policy state, projected immutable CP inputs,
ordered effects and all sorted duplicate event names after every request.
Fixture messages are serialized/deserialized through the full cereal schema on
both sides, preserving its Float32 extraction before binary64 source arithmetic.
No Python is called by the native policy executable.

## Verification and artifacts

Evidence base (local, not committed):
`.omo/evidence/selfdrived-car-specific-168/` in the #168 worktree.
The reproducible differential invocation is:

```sh
PYTHONPATH=. /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-uploader/venv/bin/python \
  rust/tools/check_car_specific.py \
  --binary /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-statsd/worktree/rust/target/debug/examples/car_specific \
  --binding /home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-startup-integration/combined-inputs-3/startup-params-binding/params_pyx.cpython-312-x86_64-linux-gnu.so \
  --output .omo/evidence/selfdrived-car-specific-168/differential-final
```

The output directory must be fresh. The original binding's neighboring
`provenance.json` records its build provenance. The final manifest hashes the
binding, native executable, owned source, authoritative source and exact input.

| Criterion | Exact scenario | Invocation | Binary observable | Captured artifact under evidence base |
| --- | --- | --- | --- | --- |
| All brands, common branches, event ordering/duplicates, complete state | 170 named scenarios, 29,787 requests; all 51 source event-add branches and both outcomes of all 79 source conditions | Differential command above | Native exit 0; every native result exactly equals the source result | `differential-final/{input,source,native,scenarios}.jsonl`, `source-coverage.json`, `manifest.json` |
| Steering warning and speed hysteresis thresholds | `steer-silent-initial-*`, `steer-unpressed-{148,149,150}`, brand hysteresis and engagement-minimum scenarios; adjacent Float32 values | Same differential command | Exact silent/loud counter, suppression, low-speed latch and event equality at each step | `differential-final/scenarios.jsonl`, `source.jsonl`, `native.jsonl` |
| Cruise/controls/brake/gear/button edges and no-entry gates | All brand/PCM/longitudinal/network combinations; every defined gear/button variant; `gm-brake20-*`, `button-enable-no-entry-category`, `common-options-*` | Same differential command | Exact event names with repeated cancel/audio/below-speed entries preserved | `differential-final/{source,native}.jsonl`, `manifest.json` |
| Settings cadence, Tesla effects, Bluetooth cancel and shutdown behavior | `mute-100-frame-read-order`, `explicit-frame-zero-settings`, `tesla-lkas-confirmed-*`, `tesla-write-error-and-retry-edge`, `shutdown-write-error-and-latch`; Bluetooth -3 rising/repeated transitions in all-brand sequences | Same differential command | Exact ordered get_bool/put_bool effects, frame100/200/300 state, ignored write error behavior and final physical key bytes | `differential-final/{source,native}.jsonl`, `params-final.json` |
| Malformed and invalid wire input | Network, current/previous gear and button ordinal65535; invalid brand UTF-8; truncated CP/state/control; uninitialized, unknown operation and missing input | Checker launches the example separately for each captured invalid input | Each exit1, typed error stderr, no result emitted for rejected request | `differential-final/invalid-*.{input.jsonl,stderr,stdout}`, `manifest.json` |
| Bounded package build | Example `car_specific` | `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=<inactive-home-cache> cargo build --manifest-path rust/Cargo.toml -p openpilot-selfdrived --example car_specific --locked -j2` | Exit0 and `Finished` | `build-final.log`, `build-final-disk-preflight.txt` |
| Existing package regression suite | Existing state, alert-manager and event-container tests | Same cache/environment; `cargo test --manifest-path rust/Cargo.toml -p openpilot-selfdrived --tests --locked -j2` | Exit0, zero failed tests | `test.log`, `test-disk-preflight.txt` |
| Strict lint and formatting gates | All package targets, all owned Rust files, all three checker modules | Same cache/environment; `cargo clippy --manifest-path rust/Cargo.toml -p openpilot-selfdrived --all-targets --locked -j2 -- -D warnings`; `rustfmt --edition 2021 --check <owned-Rust-files>`; cached Ruff `check <owned-Python-files>` | Each exit0; Ruff reports all checks passed | `clippy.log`, `fmt.json`, `ruff.log`, `validation.json` |

Available space was checked before each build. The inactive statsd/Home target
cache was coordinated with the root executor, incremental compilation was
disabled, and no cache/output cleanup occurred. Preflight and final measurements
are recorded with the validation ledger. Existing external msgq C++ bindings
still emit compiler warnings and remain an explicit native dependency; strict
Rust Clippy passed with warnings denied.

All owned production/fixture modules remain under 200 non-comment, nonblank
lines. Responsibility is separated into policy state, cereal boundary, common
policy, brand policy, binary protocol, source oracle and fixture scenarios. The
native boundary contains no unchecked dynamic JSON or enum fallbacks. The
small private common-policy entry receives the source's independently required
input snapshot, options, Params effects and event catalog; its public entry groups
options/catalog for callers. Exact original-source comparisons lock behavior,
including mutable state rather than only final event counts.

Docs-Not-Needed: this native library/host-checker slice adds no user-visible setting
or production daemon selection and changes no existing setting behavior; paired
user guides and public Wiki are outside this assignment.
