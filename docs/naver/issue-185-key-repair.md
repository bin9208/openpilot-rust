# #185: inferred exact K5 hybrid torque-key repair

2026-10-23. Assigned under #497, only in
`/home/fma1/.codex/worktrees/backlog-fixes-497/openpilot-rust` on
`codex/fix-497-stale-runtime-defects`. Source history anchor and native RED
baseline: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.

The parent audit selected this narrow inferred key repair after independent
source review under the user's request to resolve neglected defects. It is **not independently verified calibration or vehicle
acceptance**. Hosted RED/GREEN, full existing regression gates and device/drive
acceptance remain open. No local functional execution, tests, builds, fixture
generation, device connection, commits, pushes or GitHub mutations were performed.

## Evidence, inference and exact change

The fork declares exactly one complete Kia/K5/DL3/2024/HEV identity:
`KIA_K5_DL3_24_HEV`, documented as “Kia K5 Hybrid 2024” and mapped from legacy
“KIA K5 HYBRID 2024 (DL3)”. Its CAN-FD platform specifies mass1553, wheelbase2.85,
steerRatio13.27 and tireStiffnessFactor0.5. The existing params table has an orphan
`KIA_K5_DL3_HEV_24 = [2.5, 2.5, 0.1]`: identical tokens with year/powertrain reordered.
There is no other enum or substitution using that spelling. Other K5 identities
have different years, powertrains or legacy/CAN-FD configurations. The adjacent
petrol DL3 row is not used as the authority for this repair.

Both spellings already coexist in parentless root
`ecf4d58ce5df4d704fed1aeb942dd3454a388f52`. Root object metadata contains no parent;
`git log --all -S` and blame reveal no accessible rename chain. Thus **author intent
is inferred from a unique exact identity match**, not proved by rename history.
The existing fork row has no independent measurement/EPS-variant annotation;
its placement in `params.toml` alone does not prove measured calibration quality.

Only the source `substitute.toml` adds:

```toml
"KIA_K5_DL3_24_HEV" = "KIA_K5_DL3_HEV_24"
```

This uses the existing resolver, preserving the original row, enum and fingerprint
names. There is no normalized-name resolver, generic fallback, new numeric tune,
or calibration transfer from a nearby vehicle. Enabling the formerly blocked
parameter/control path still requires runtime and vehicle acceptance evidence.

## Regression definitions and exclusive expectation updates

Tests were written before adding the alias. New public API test
`rust/crates/card/tests/hyundai_torque_key_497.rs` constructs real full Hyundai
parameters in 64 cases: offsets0/4, HDA2 off/on, torque/angle selection, NNFF
off/on, alpha-long off/on and EPS firmware absent/present. It asserts the exact
existing torque values where used, finite physical scalars, correct safety masks,
NoOutput leading safety for second Panda, bus selection, longitudinal/PCM policy,
unchanged exact candidate identity and no unexpected NNFFModelName side effect.
Optional `HYUNDAI_TORQUE_KEY_EVIDENCE` exports complete native cereal messages.

New `rust/tools/check_hyundai_torque_key_497.py` executes the unchanged baseline
resolver AST with all three immutable baseline TOML files, then invokes public
`CarInterfaceBase.get_std_params`: the original exact KeyError is required.
`--baseline-red` exposes the real nonzero failure for hosted RED capture. No
production source file is replaced. The normal control checks unique enum token
identity, original row preservation and the current exact alias, executes real
source full CarParams for the same64 cases, and, with `--native`, compares every
schema field exactly. Source Params/NNFF writes are asserted empty in this matrix;
no numerical tolerance or broad output waiver is added.

Existing exclusive HKG expectations now require success and fresh corpus coverage:

- `card_hyundai_source.py` requires zero source parameter errors and KIA inclusion;
  `--baseline-defect` retains immutable baseline failure through the new control.
- `hyundai_parameters.rs` replaces its old MissingTorque assertion with public-API
  success and requires the exact candidate in the full source matrix.
- `check_card_hyundai_params.py` retains complete dictionary equality, requires KIA
  coverage, and reports the actual now-empty source parameter error list.
- `card_hyundai_state_source.py` appends two KIA scenarios (stock-long/HDA1 and
  OP-long/HDA2), preserving every old scenario index. Each has320 real continuous
  state updates; synthetic ACCELERATOR_ALT supplies the existing fingerprint-based
  hybrid detection rather than changing vehicle flags in production.
- `card_hyundai_controller_source.py` consumes those actual state scenarios through
  its existing320-update control loops, requires both KIA scenarios and nonempty
  CAN outputs. Existing complete state/actuator/CAN comparisons remain strict.

## Parent-owned shared integration deltas

Do not duplicate these edits in another worker. The exact required native catalog
row is candidate `KIA_K5_DL3_24_HEV` only, replacing `torque:null` with:

```json
{"LAT_ACCEL_FACTOR":2.5,"MAX_LAT_ACCEL_MEASURED":2.5,"FRICTION":0.1}
```

Keep every other field unchanged. This delta and factory cases were observed
already being applied by the parent/Mendel while this worker was authoring; they
were not edited here. Required factory cases: exact candidate with `[]` and
`[(4, [(0x1cf, 8)])]`, appended to preserve old case identities. Run them through
normal factory stdout and constructor checks, not a seeded substitute interface.
Shared provenance must identify an inferred fork key repair, not primary measured
calibration. Parent owns that provenance, shared audit and paired user guides.

`rust/crates/card/tests/vehicle_params.rs` still contains the old assertion that
this candidate fails. Parent must replace only that KIA expectation with success
(max lateral acceleration2.5), retaining unknown/missing-identity rejection.
`tests/registry.rs` is also parent/Mendel-owned; its normal brand parameter case
should include this exact candidate (already observed appended). No shared test,
catalog, factory, provenance, CI or guide file was edited by this worker.

## Hosted commands (not executed locally)

With the existing pinned oracle dependencies from repository root, and the source
baseline Git object available:

```sh
export PYTHONPATH="$PWD:$PWD/opendbc_repo:$PWD/rust/tools"
export HYUNDAI_TORQUE_KEY_EVIDENCE="$RUNNER_TEMP/kia-key-185/native"
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-card --features native --test hyundai_torque_key_497
python rust/tools/check_hyundai_torque_key_497.py --native "$HYUNDAI_TORQUE_KEY_EVIDENCE" --output "$RUNNER_TEMP/kia-key-185/comparison"
```

The control must report64 complete schema comparisons. Native catalog changes
must be present before GREEN. Baseline-only source RED command (expected exit1,
actual KeyError naming only this candidate, not an import/setup failure):

```sh
python rust/tools/check_hyundai_torque_key_497.py --baseline-red --output "$RUNNER_TEMP/kia-key-185/source-red"
```

For native behavioral RED, in a **separate hosted baseline checkout** retain the
new public test, restore only `rust/crates/card/data/vehicle.json` from the stable
baseline, and run the same focused test. Require MissingTorque at the public API
`expect`, not a compilation/setup error. Parent owns orchestration and restoration;
never restore production files in the shared authoring worktree.

Regenerate ordinary HKG fixtures through the existing stage pipeline/DBC bootstrap
(`card_qa.ci.source_command`) rather than reusing captures with the old missing-key
result. Retain the existing Hyundai full checks, e.g. after fresh generation:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-card --features native --test hyundai_parameters --test hyundai_state --test hyundai_controller
python rust/tools/check_card_hyundai_params.py "$HYUNDAI_FIXTURE_DIR"
python rust/tools/check_card_hyundai_state.py "$HYUNDAI_FIXTURE_DIR"
python rust/tools/check_card_hyundai_controller.py "$HYUNDAI_FIXTURE_DIR"
```

The added state/control scenarios contribute640 actual state and640 controller
ticks to the existing exact comparisons. Parent registers new commands and keeps
factory, firmware, NNFF, full runtime, ARM, safety and required aggregate gates.
No hosted results or vehicle benefit is claimed by this document.

Local evidence is limited to Rust formatting, Python AST/TOML syntax and scoped
diff review/check. The original params table was not edited. Full software and
vehicle acceptance remain open until their respective evidence exists.
