# Native controlsd and consumed control policies (#150)

Tracking: [#150](https://github.com/bin9208/openpilot-rust/issues/150), full-runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1). Source baseline: `42d81e3b68634213b5889f8f023825cff19e33b6`. Implementation base also includes msgq creation fix `9939f81626cd82224837dbbb13f7aad0e55a7f20` (#152). Status: implemented native component with local source/IPC/memory validation; integration CI and whole-runtime/device acceptance remain open.

## Source boundary

Rust owns `controlsd.py`, PID/MultiplicativeUnwindPID, consumed VehicleModel operations, angle/PID/torque lateral control, NNFF/NNFFLite, LongControl and consumed cruise-coasting helpers, drive/steer-ratio helpers, Pose/PoseCalibrator transforms, CarrotControls suspension and Carrot message freshness. It consumes actual CarParams and the existing 16 SubMaster topics, publishing controlsState then carControl. The source polls selfdriveState with a 15 ms timeout and monitors a 100 Hz clock without adding Ratekeeper sleep. TICI core6/FIFO53, existing limits, Params cadence and publication validity remain unchanged.

The complete source interface registry contains 279 fingerprints across body, chrysler, ford, gm, honda, hyundai, mazda, mock, nissan, psa, rivian, subaru, tesla, toyota and volkswagen. Runtime policy selection uses fingerprint identity, as source interface lookup does; a mismatched or unknown fingerprint must not silently choose a generic policy. Physical vehicle constants remain actual CarParams values, never fixture profiles.

Every consumed interface override found before implementation:

- GM: acceleration bounds; Volt/Volt-CC steering-angle feedforward; linear, sigmoid-plus-linear and NanoFF neural torque selection. NanoFF weights are in `torque_data/neural_ff_weights.json` (Bolt EUV, Bolt CC and Volt).
- Honda: Bosch acceleration bounds versus Nidec cruise-speed interpolation.
- Ford: cruise-speed acceleration interpolation.
- Toyota: acceleration maximum selected by `RAISED_ACCEL_LIMIT`.
- All other identities inherit the source base acceleration bounds, steering feedforward and linear torque/friction policies.
- Common interface startup: EPS firmware byte representation, difflib SequenceMatcher model selection over all 117 shipped Flux models, unconditional model initialization, comma-NN precedence, NNFF/NNFFLite short-circuit Params reads and LongitudinalPersonalityMax write.
- Constructor-only Params effects: Hyundai camera-SCC hint reset and related reads; GM AutoEngage and VCruiseCarrot initialization reads; Tesla safety-VM initialization read. Unconsumed CarState/CANParser/CarController algorithms remain card scope; their observable startup Params effects are retained without a hidden Python constructor.

Additional controlsd/controller branches include Hyundai fixed longitudinal tuning, Toyota acceleration-error PID, Tesla-only standstill steering permission, VW MEB flag curvature PID/HUD, live torque adaptation, custom torque parameters, lane-line smoothing, soft hold, stopping/start/override transitions, coasting vetoes, lateral suspension, safety feedback and nonfinite actuator sanitation. Existing source peculiarities, including Float32 log-field readback before PID and pre-publish empty CC pose fields, are part of the comparison contract.

## Numeric contract fixed before implementation

Float64 calculations use absolute plus relative `1e-9`; cereal Float32 outputs use absolute plus relative `2e-6`. All control branches, saturation, controller state, validity, selected identity/model/policy, Params actions, ordering and serialized discrete fields are exact. Nonfinite classifications agree. Tolerance never excuses a control-decision difference.

FluxModel deliberately stores and calculates in Float32; NanoFF uses NumPy Float64. Float32 source operations must be preserved rather than described as Float64-precision arithmetic. Initial NN acceptance targets bit-identical Float32 model outputs and the fixed Float64 budget after conversion, with all policy decisions exact. No observed failure authorizes increasing these budgets. The approved source-pinned NumPy 2.5.3/OpenBLAS 0.3.34.106.0 native artifact may supply BLAS primitives through a checked Rust-owned boundary; Rust owns weights, selection, normalization, activation orchestration and controller state. Any selected kernel and its provenance must be recorded before completion. Python/NumPy execution is oracle/development-only.

## Required evidence

Actual unchanged-source policy/model comparisons cover every vehicle family and neural asset, seeded long-running transitions, malformed/stale/disabled/restart/boundary inputs, full controlsd loop and exact Params actions. Native process evidence uses owned synthetic IPC/Params and validates startup, both publications, persisted actions, signals and no Python fallback. New native boundaries require memory/sanitizer evidence; safe Rust requires meaningful tests and Miri. Candidate catalog/inventory, host/ARM/required CI and post-merge validation remain explicit stages. No physical scheduling, device/C3X/NAS/account access, real CAN or production selection is authorized; full project startup/log upload and first user device comparison remain open under #1.

## Candidate and dependency contract

`openpilot-controlsd` runs continuously with actual CarParams, Params and msgq. `--frames N` is an optional positive host-test bound. It accepts `--assets DIRECTORY` (default `opendbc/car/torque_data`) and `--numerics DIRECTORY`; the numerical directory otherwise comes from `CONTROLS_NUMERICS` or the executable sibling `controlsd-numerics`. The normal launcher already exports `OPENBLAS_NUM_THREADS=1`. Missing numerical artifacts or malformed inputs fail explicitly; there is no Python fallback.

The numerical artifact is the same source-locked GNU NumPy 2.5.3 artifact documented and staged by [torqued](torqued-validation.md#native-numerical-dependency). Rust calls checked `scipy_cblas_sgemv64_`, `scipy_cblas_sdot64_`, `scipy_cblas_dgemv64_` and `scipy_cblas_ddot64_` operations through an owned library handle. Buffers and dimensions are checked before calls. The artifact is trusted executable code: manifest hashes verify packaging consistency, not authenticity of an arbitrary supplied library. The GNU ARM dependencies include compatible libc/libm, libgcc and libz. These checks are not AGNOS ABI or device validation.

The source Float32 exponential uses NumPy's AVX2/FMA polynomial on capable x86_64 hosts and its scalar expf path on the tested ARM build. The polynomial preserves source coefficients and rounding order; the NumPy BSD license is retained in `rust/crates/control-policy/NUMPY-LICENSE.txt`. ARM comparisons execute the actual aarch64 NumPy 2.5.3 wheel and native binary under QEMU, rather than comparing ARM to a different host numerical kernel.

Controller details preserved by source comparisons include torque PID state retention while inactive, the custom-torque `>1` reset condition, Float32 torque-error readback, Tesla's `DisableMinSteerSpeed` constructor read, Hyundai CAN-FD bus constructor reads, MEB's pre-assignment `leadLimiting` value, and NumPy division behavior for a zero neural jerk horizon. The consumed torque controller receives empty CarControl pose fields before publication, so its pitch branch is not exercised by controlsd. Unconsumed general-purpose VehicleModel and CAN-control algorithms are not claimed ported by this component.

## Reproduction

Use the repository's locked Python oracle dependencies (NumPy 2.5.3, pycapnp 2.1.0 and the existing source import dependencies), generated DBC files, Cargo 1.94.0, and a staged native numerical artifact. Check disk space before building; preserve the 25 GiB floor plus expected growth. Set `CARGO_INCREMENTAL=0` and use bounded `-j2` builds. `TARGET` below denotes the coordinated Cargo target directory and `EVIDENCE` an ignored output directory.

```sh
export OPENBLAS_NUM_THREADS=1 CARGO_INCREMENTAL=0
export PYTHONPATH=.:rust/tools
export CONTROLS_NUMERICS=/path/to/staged/numerics
cargo build --manifest-path rust/Cargo.toml -p openpilot-control-policy -p openpilot-controlsd --bins --examples --locked -j2
python rust/tools/check_control_neural.py --trace "$TARGET/debug/examples/neural_trace" --numerics "$CONTROLS_NUMERICS" --evidence "$EVIDENCE/neural"
python rust/tools/check_control_policies.py --trace "$TARGET/debug/examples/policy_trace" --evidence "$EVIDENCE/policies"
python rust/tools/check_controlsd.py --trace "$TARGET/debug/examples/controlsd_trace" --numerics "$CONTROLS_NUMERICS" --evidence "$EVIDENCE/loop"
python rust/tools/check_control_failures.py --trace "$TARGET/debug/examples/controlsd_trace" --numerics "$CONTROLS_NUMERICS" --evidence "$EVIDENCE/failures"
python rust/tools/check_controlsd_daemon.py --target "$TARGET" --numerics "$CONTROLS_NUMERICS" --evidence "$EVIDENCE/native"
cargo test --manifest-path rust/Cargo.toml -p openpilot-control-policy -p openpilot-controlsd -p openpilot-manager-catalog --all-targets --locked -j2
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-control-policy -p openpilot-controlsd -p openpilot-manager-catalog --all-targets --locked -j2 -- -D warnings
```

`controlsd_source.py INPUT OUTPUT` runs only the unchanged-source oracle. `controlsd_compare.py --evidence DIRECTORY` compares an existing `source.json` with native `native-raw.json`, allowing the same full-loop contract to run under an ARM Python/QEMU environment. `control_neural_source.py INPUT OUTPUT ASSETS` similarly evaluates unchanged FluxModel/NanoFFModel definitions without requiring unrelated controller imports. The full source result retains raw constructor stdout. Comparison excludes only the CAN parser's `DBC: ...` and `Using Hyundai CAN FD checksum` initialization diagnostics; consumed control logs, Params actions and outputs remain checked.

Registry regeneration uses `generate_control_registry.py --output PATH`; compare both the generated registry and provenance JSON with `rust/crates/control-policy/data/`. This prevents a new source identity or override from silently becoming a generic runtime fallback.

## Captured local evidence

Evidence is retained in the parent workspace's ignored `.omo/evidence/controlsd-150/`. `INDEX.json` records exact invocations, binary/source hashes and nonempty artifact paths. Local results are intermediate component evidence, not completion of the whole runtime.

| Scenario | Binary observable | Artifact |
| --- | --- | --- |
| Full host control loop, 30 cases and 8,605 frames | 701,778 numeric comparisons exactly equal; Params actions, state, flags and both publications agree | `final-loop/results.json` |
| All registered identities | 279 identities, 29,295 consumed policy inputs, 1,116 model selections and 280 SequenceMatcher pairs exactly equal | `policies/results.json` |
| Neural assets on x86_64 and aarch64 | 117 Flux models, 3 Nano models, 66 inputs each and 4,102 exp inputs; bit-identical results | `neural/results.json`, `arm-neural/results.json` |
| ARM full loop under QEMU | 30 cases / 8,605 frames / 701,778 numeric comparisons exactly equal; no device or physical scheduling claim | `final-arm-loop/results.json` |
| Native startup, persistence and restart | Waits for CarParams; 140 frames / 280 publications per run; durable personality/hint writes; restart from saved Params; SIGINT/SIGTERM exit zero; no Python mapped | `final-native/2/results.json`, `final-native/15/results.json` |
| Malformed boundaries | 15 source-rejected scenarios also terminate natively without a control publication trace | `final-failures/results.json` |
| Native numerical memory boundary | 128 library create/drop cycles; checked matrix layouts and bad dimensions; ASan reports no errors | `asan-native.log` |
| Scheduler ABI and policy | Initialized FIFO53 ABI arguments; PC bypass; FIFO-before-affinity order and failure propagation; ASan reports no errors | `asan-scheduler.log` |
| Safe Rust state/memory | Five policy and five controller tests pass Miri strict provenance, symbolic alignment and preemption | `miri-policy.log`, `miri-state.log` |
| Build and static checks | Bounded host and GNU aarch64 builds, package tests, clippy, formatting and Python checks | `runtime-build.log`, `arm-daemon-build.log`, `unit.log`, `clippy.log`, `ruff.log` |

The native numerical test deliberately requires `CONTROLS_NUMERICS`; it does not skip when the artifact is missing. ASan instruments the Rust boundary, not external OpenBLAS internals. Miri does not execute native libraries or kernel scheduling. The scheduler test uses a captured callback instead of applying FIFO scheduling to hardware.

## Source defect and remaining integration

[Issue #156](https://github.com/bin9208/openpilot-rust/issues/156) records two pre-existing PSA gaps: `get_non_essential_params('PSA_PEUGEOT_208')` raises on a missing torque-data key, and the actual PSA constructor additionally requires absent `psa_aee2010_r3.dbc`. No source policy or DBC is fabricated here. All 15 families' consumed policies are compared directly; full unchanged-source loop coverage spans the other 14 families. The PSA source startup limitation remains open.

The candidate catalog and `rust/port-status.json` expose the native component while preserving original production descriptors. Exact-SHA CI, integration with selfdrived/card/planners and the rest of the project-owned runtime, normal startup/log upload, AGNOS packaging and first user device comparison remain separate gates under #1. No C3X, NAS, real CAN, account service or physical device was accessed. This work establishes neither CPU savings nor vehicle behavior.
