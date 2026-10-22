# Vehicle backlog fixes #497: source/native fixes and explicit remaining boundaries

Status: implementation and regression definitions prepared; **cloud execution pending**.
Worktree: `/home/fma1/.codex/worktrees/backlog-fixes-497/openpilot-rust`.
Branch: `codex/fix-497-stale-runtime-defects`; baseline
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
No local functional test, build, fixture generation, device operation, GitHub
mutation, commit or push was performed by this vehicle worker. The local checks
are Python AST parsing, Rust 1.94 formatting/parsing and scoped diff review.
Passing baseline Actions are historical evidence, not validation of this patch.
Parent owns workflow registration, paired user documentation and all cloud runs.

## Deliberate source/native corrections

### #186: scalar CANFD alternate-button captures

`CarState` copies `CANParser.vl`, whose values are scalars. The old controller
indexed each scalar as `value[0]`, caught the exception and discarded the entire
template. The source controller now copies the scalar mapping before mutating
its button/counter values. No list-valued alternate capture producer is added.

The native controller now owns the corresponding template and sends alternate
cancel/speed/resume frames through the existing `alt_buttons` packer. Fresh
captures reset the count, missing fresh captures retain the owned template, and
each speed-send operation increments the count once even when the same frame is
repeated. The existing mutating counter rule `(COUNTER + 1 + count) % 256`,
HDA2 ECAN versus other CAM selection, second-Panda offset, 0.25 s cancel window,
spam cadence, brake/hold/parking interlocks and non-alternate branches remain.

`vehicle_backlog_497.rs` covers scalar cancel, counter rollover, HDA2 and offsets
0/4, missing template, brake/hold/parking guards, repeated speed frames, owned
template reuse, captured physical-button suppression and explicit resume cadence.
`card_hyundai_alt_buttons_defect.py` now loads the immutable baseline controller
and the repaired current controller against the same real CarState capture from
the staged Hyundai corpus. It requires the old missing-frame/count failure and
the current correctly addressed cancel without mutating the producer's capture.
The existing complete Hyundai controller oracle must be regenerated and compared;
the old failing output is not an expected result for the repaired runtime.

### #188: Tesla cancel events and Cap'n Proto list replacement

The source materializes existing button events into owned dictionaries before
replacing their Cap'n Proto list. Rust removes the deliberate
`buttons.fill((Unknown, false))` reproduction of the original aliasing defect.
Cancel-state detection, the four-frame lifetime, event order and infotainment
gesture policy remain unchanged. Actual State/CAN regressions check simultaneous
cancel/infotainment press and release, with and without the vehicle bus.

### #189: actual MQB first and continuous updates

The source imports NumPy normally. No oracle injects an `np` global. The native
state owner replaces its deliberate `InheritedMqbNumpy` error with the already
defined source mean/Kalman, steering/driver-pressure/HCA, ESP, belt, cruise,
stock-message and button-update path. Four stored cereal Float32 wheel values
are converted to Float64 before the mean, then stored back as Float32 before the
speed filter, matching the source's observable rounding boundary.

The standalone Rust regression constructs all **23 MQB identities**, using
manual/automatic/direct-drive parameter detection, and requires an ordinary
empty-CAN first update followed by 32 actual wheel/steering/button CAN updates.
The original/native complete oracle adds 23 identities × 2 longitudinal modes ×
2 network locations × 3 transmission fingerprints × 96 updates. It compares
complete state/actuator fields, CAN bytes, parser and owner histories, diagnostics
and Params effects with existing tolerances. First-frame standstill, later
nonzero speed, all 96 rows and the final owner frame are mandatory. PQ/MEB
scenarios and existing fault/ownership assertions remain.

The CLI spelling `--op mqb_failure` is accepted as a compatibility alias for
`mqb_runtime`: it now requires a successful continuous run, not an exception.
The old exception remains a separate immutable source failure control in
`check_vehicle_source_fixes_497.py`. That script also records Tesla's old event
loss versus the current source and preserves partial output on failure.

## Cloud handoff

Use the existing Card job's fresh staged assets and original bindings. Do not
reuse frozen expected controller output from before these source corrections.
The Rust regression uses `HYUNDAI_DBC_DIR` when set; its fallback is the ordinary
source DBC root. The generated Hyundai DBC must exist before running it.
Ensure the exact baseline Git object is available to all `git show` failure
controls; a shallow checkout needs
`git fetch --no-tags origin b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` in Actions.

```sh
# In Actions only, after the existing stage_card_fixtures/build steps:
cargo test --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --test vehicle_backlog_497
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_vehicle_source_fixes_497.py --output "$RUNNER_TEMP/vehicle-497/source-controls"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_alt_buttons_defect.py "$HYUNDAI_FIXTURE_DIR" "$RUNNER_TEMP/vehicle-497/hyundai-controls"

# Existing original/native comparison binaries and pinned numerical dependency:
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_volkswagen.py --binary rust/target/debug/examples/volkswagen_trace --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-497/mqb" --op mqb_runtime
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_tesla.py --binary rust/target/debug/examples/tesla_trace --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-497/tesla" --op runtime
```

For the Hyundai control command, run through the existing
`card_qa.ci.source_command(script, args, HYUNDAI_DBC_DIR)` bootstrap so the
original parser uses the fresh generated DBC directory. Its output parent must
exist. The same bootstrap is suitable for the source controls. Preserve native
command exit status and upload partial evidence on failure.

Then retain the existing full `cargo test -p openpilot-can -p openpilot-card
--features openpilot-card/native --locked`, strict Clippy, Hyundai complete
parameter/state/controller postchecks and `check_card_ci.py --phase reference`
and `--phase ipc` gates. These commands are **requested**, not reported passed.
Affected native ARM builds/source comparisons must pass separately. No added
test proves physical CAN, vehicle/road acceptance or CPU savings. The source
controls explicitly authenticate the immutable baseline code with SHA-256;
native regression RED execution on the baseline belongs to an isolated Actions
checkout, never by replacing files in this shared worktree.

## #178: authoritative MLB algorithm repair

Primary source is pinned to
[`commaai/opendbc@c3955753ae7258412814e2cdca3ee8af979e4679`](https://github.com/commaai/opendbc/commit/c3955753ae7258412814e2cdca3ee8af979e4679).
The [MLB caller/table](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/volkswagen/mlbcan.py)
blob is `dc3b717e9cde7265d2dff13a9372a673c01fe70d`; the
[XOR helper](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/volkswagen/mqbcan.py)
blob is `67dd9da71aab09a1332072aef1b2228e3769a3ba`.
The preserved local caller already has the ten exact seeds. Its source helper now
accepts the authoritative optional `initial_value=0`; Rust implements that seeded
XOR excluding the signal's checksum byte. Unlisted MLB addresses retain MQB CRC
fallback. MQB/MEB/GEN2/PQ and all other checksum families retain their algorithms.

`rust/crates/can/tests/data/mlb_497.json` contains **independent literal synthetic
source-backed vectors, not vehicle captures**. For each of ten IDs, four payloads
have non-checksum XORs `00`, `21`, `7f`, `bf`; the literal expected byte is that
value XOR the primary seed. Inputs include zero bytes, a nonzero counter nibble,
nonzero tail and checksum positions 0, 2 and 7. Changing the checksum byte must
not change the result. No test computes its own expected checksum using the
implementation under test. The native regression covers all 80 checksum cases;
the existing actual packer regression now requires literal `08 00 00 00 00 00 00 00`.

`check_mlb_primary_497.py` downloads both exact primary files in hosted CI,
authenticates their Git blob SHA before executing their **original function ASTs**,
and compares primary, maintained source, native and the literals. It also requires
40 actual baseline four-argument TypeErrors and four PQ/default-helper controls.
Network/blob mismatch fails; there is no locally rewritten primary oracle or
network fallback. The complete CAN codec/lifecycle/checksum jobs must also pass;
the repaired MLB lifecycle is no longer skipped as an expected source TypeError.
Existing unrelated FCA vector rejection remains strict.

Keep #178 open pending exact-head cloud evidence and any required independent
vehicle/known-good capture acceptance. Synthetic source-backed vectors establish
an algorithm regression boundary and do not supply physical acceptance.

## #180: exact PSA asset, generic codec only

Imported [primary PSA DBC](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/dbc/psa_aee2010_r3.dbc)
blob `fb9a0c5159a7501a83a447234761ce58842f6eeb` byte-for-byte: 56,757 bytes,
108 message declarations, SHA-256
`e9791b8603861a86b2f2acc93e6327ef82587ce84f28d1438a1338d35cce5db5`.
The asset header is unchanged; the existing `opendbc_repo/LICENSE` matches the
pinned primary license. Adjacent `psa_aee2010_r3.provenance.json` records admission.

`check_psa_asset_497.py` requires all 13 messages/30 signals actually referenced
by this fork's PSA state and steering dictionary (read from their ASTs), compares
the complete generic codec and real parser histories, and exercises the actual
source `create_lka_steering` caller with inactive/active signed angles.
`psa_asset_497.rs` exercises native pack/parse with those signed angles. The full
catalog gate automatically discovers the new DBC. All are cloud-only recipes.

**This is not a PSA runtime port.** Normal source `get_params` still fails on
missing torque; direct source State still calls missing `parse_wheel_speeds`.
Candidate-only source construction can now load its DBC. Native construction
validates that asset then explicitly returns `RuntimeUnported`, before Params
effects. Missing-file rejection is retained in a separate absent-directory test.
The source/native difference at candidate-only construction is documented in the
boundary checker, rather than claiming a missing asset still exists. The complete
state/controller port and full parameter admission remain outside this batch.

## #185: exact Ford and Model X admissions; three remaining failures

[Primary overrides](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/torque_data/override.toml),
blob `f406ae64da4f2fa5a6b50e42b87feb2c7561d16b`, provide exact rows:
`PSA_PEUGEOT_208 = [nan, 2.0, nan]`, `TESLA_MODEL_X = [nan, 2.5, nan]`,
`FORD_ESCAPE_MK4_5 = [nan, 1.5, nan]`, `FORD_EXPEDITION_MK4 = [nan, 1.5, nan]`.
The **two Ford rows and Model X row** are admitted to the source override and native catalog.
NaNs remain official placeholders (native JSON null), never measured friction or
LAT_ACCEL_FACTOR. Existing Ford brand parameter logic selects angle control;
source and native finish skip torque tuning even when NNFF is requested. Model X also retains its existing angle selection and dashcamOnly restriction.

`ford_override_497.rs` checks real full parameter construction, finite control
scalars, Angle/PID selection and no NNFFModelName effects with NNFF off/on.
`check_vehicle_source_fixes_497.py` requires finite complete source CarParams,
unchanged raw NaN placeholders, the same angle union and actual State/Controller
construction. The complete Ford oracle now includes both vehicles, 520 real
updates each, with NNFF off/on. The factory comparison includes both while
preserving existing case identities. The old negative parameter regression now
requires the **same SecOC dashcamOnly rejection**, not a missing-data error.
No safety/firmware/size checks, steering limits or bus policy are weakened.
`rust/crates/card/data/vehicle-497-provenance.json` records this scoped admission.

Keep #185 open for the three remaining exact identities:

- PSA: source `parse_wheel_speeds` and native state/controller are unimplemented.
  Importing its placeholder now would require a larger runtime admission repair.
- `GMC_YUKON_CC` and `KIA_K5_DL3_24_HEV`: no exact key in primary override,
  [measured params](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/torque_data/params.toml)
  (blob `1fbabf6f76ba4bbc4717fb2e9797a4ef55616736`) or
  [substitutions](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/torque_data/substitute.toml)
  (blob `161c3792c4d3829e3bf3a6c4fda8c5dfe61742bb`). The local near spelling
  `KIA_K5_DL3_HEV_24` is not authority to alias another identity.

## Model X follow-up: correction of the DBC claim and safe admission

The earlier claim that Model X lacks a party DBC was **mistaken**. The generic
catalog generator chooses `Bus.main`/`Bus.pt` for `dbc_pt`; that field is null for
Model 3 and Y as well. It does not represent Tesla's `Bus.party` mapping.
[Primary TeslaPlatformConfig](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/tesla/values.py#L39)
(blob `6da33ecb19496435e546e3afdcfdec7e0da3104b`) supplies
`{Bus.party: 'tesla_model3_party'}` through its actual dataclass default_factory,
which applies to Model X's two-argument constructor. The actual fork's
[values](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/tesla/values.py#L43)
(blob `5b2b3f868a49873dcada96da992a7d10683c4d35`) and
[base configuration](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/__init__.py#L379)
(blob `9202ab152e2e6f4e938e6870ca6db850dd33a309`) retain that contract:
`__post_init__` only calls the unchanged no-op `init` method.

The exact official `[nan, 2.5, nan]` row is now added to the preserved source
and native catalog; the placeholders are not measured factors. No Tesla runtime
algorithm, DBC asset, generic `dbc_pt`, radar mapping or safety policy changes.
The existing source/native parameters select angle and **dashcamOnly**; Model X
has no radar DBC even when a 0x410 fingerprint appears. Missing DAS settings,
optional vehicle-bus parser, safety bits, all angle limits and the source's Model Y
vehicle model for safety limiting remain. This is dashcam runtime admission,
not new active driving support or physical CAN acceptance.

Hosted regression definitions (not run locally):

- `tesla_params.rs`: 32 NNFF/longitudinal/vehicle/DAS/radar combinations require
  finite angle CarParams, dashcamOnly, the exact Tesla safety bits, no NNFFModelName
  write, and no FSD admission from Model 3 firmware. A separate missing-directory
  constructor control still requires the existing party DBC.
- The full Tesla oracle now includes 64 Model X fullParams cases and eight real
  State/Controller cases, **620 updates each**, spanning NNFF off/on, longitudinal
  off/on and vehicle bus absent/present. Existing Model 3/Y fixtures retain their
  identities, generated bytes and random sequence. Full wire fields, owner state,
  ordered CAN, logs, cadence and Params effects remain exact comparisons.
- `check_tesla_model_x_497.py` executes the actual dataclass default, full current
  parameters, real CarState/Controller construction and 33 normal first/continuous
  updates for each of eight combinations. It asserts absence of a radar parser
  even with a radar fingerprint. The complete original baseline interfaces module
  and three immutable baseline torque files supply the genuine KeyError control;
  only data-file paths and Params are owned boundaries, with original bodies and
  input hashes retained. No injected `np` or replacement vehicle policy is used.
- Factory comparisons add ordinary and optional vehicle-bus Model X construction.
  The registry regression now accepts its full parameters while asserting that
  dashcamOnly/radarUnavailable remain set.

Exact added registration and focused compile regression commands in Actions:

```sh
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_tesla_model_x_497.py --output "$RUNNER_TEMP/vehicle-497/model-x-controls"
cargo test --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --test tesla_params --test registry --test ford_override_497
```

Retain the existing full `check_card_tesla.py` and factory commands; their new
cases are automatically included. Fetch the immutable b555 baseline object for
source controls when checkout is shallow. The first hosted vehicles attempt at
`4072836f430365d1851e65bf2d6b15ca820fc1e1` stopped compiling at the Ford test's
extra `get_lateral_tuning().unwrap()`. It is corrected to the Reader getter API in
both Ford and new Model X tests. No functional PASS is inferred from that run.

Model X checkpoint paths: `opendbc_repo/opendbc/car/torque_data/override.toml`,
`rust/crates/card/data/{vehicle,vehicle-497-provenance}.json`,
`rust/crates/card/tests/{tesla_params,registry,ford_override_497}.rs`,
`rust/tools/card_qa/tesla/scenarios.py`, `rust/tools/check_card_tesla.py`,
`rust/tools/check_card_factory.py`, `rust/tools/check_vehicle_source_fixes_497.py`,
`rust/tools/check_tesla_model_x_497.py`, and this document. No workflow,
`check_card_ci.py`, daemon or common baseline-proof helper is changed here.

## PSA #501 approved design, not implemented at this checkpoint

Dedicated follow-up: [#501](https://github.com/bin9208/openpilot-rust/issues/501),
with existing asset #180 and placeholder #185 acceptance retained. After the
parent's Model X selective-commit coordination signal, implement the exact
[primary speed helper](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/interfaces.py#L287)
(blob `34d557e5b535f3333ed1101a747c8d68f08a3ab1`): sum raw wheels, divide by four,
multiply by unit then wheelSpeedFactor, store cereal vEgoRaw, then pass the stored
value to the existing Kalman filter. No wheelSpeeds field or rounding policy is
invented. The actual helper has two body lines and is missing only from this
vendored base.

The bounded implementation covers the one declared PSA identity, exact official
`[nan, 2.0, nan]` angle placeholder, existing source 3-bus state, shared wrapper,
20Hz steering controller/status 2→3→4 and full registry lifecycle/diagnostic
ownership. It preserves dashcamOnly, PSA safety, radarUnavailable, no longitudinal
CAN and all steering/rate/intervention limits. Source/native and actual IPC
comparisons, negative baseline/missing-asset controls and full hosted rig gates
are required before functional completion; physical CAN remains user-owned.
No PSA catalog/override/base/registry implementation edit is part of this
Model X checkpoint. Parent owns workflow/command registration and shared guides.

## #181: authoritative conflict remains open

The primary [literal checksum corpus](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/can/tests/test_checksums.py)
(blob `54231fa21cc518a7533e039d92f91693afcd9d91`) still contains `7bf0026e`;
the [Giorgio function](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/chrysler/chryslercan.py)
(blob `f642c67116a93dd51d5531aaabadd9c862fcd375`) retains address `0x122`
XOR `0xF1`. Its test derives expectations from parser state before repacking;
it does not independently resolve the recorded `6e` versus computed `e1` conflict.
No CRC change or rejection weakening is made. Authoritative raw-frame provenance
or a demonstrated source algorithm correction is still needed.

## Actual fork provenance follow-up (2026-10-23, read-only)

The user has no independent data beyond the original GitHub fork. This follow-up
therefore inspected that fork directly; it changes this evidence document only.
Existing safe implementations and their cloud recipes remain independent.

### Source identity and history boundary

Pinned actual [`ajouatom/openpilot:carrot-wip`](https://github.com/ajouatom/openpilot/commit/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805)
to `7e371b92fd877c2cd160c92b4cf6fc62fe6d8805` (committer 2026-10-22).
Its `opendbc_repo` is an ordinary vendored tree
`6621677bbaf8c113f3b5989dc1021650de6eb951`, not a submodule. The `opendbc`
symlink points to `opendbc_repo/opendbc` (blob
`7cd9a5bd1ebf2e9138bf09290a2bfe9d7e251b7d`). Root/nested `.gitmodules`
are absent, and the root path history query for `.gitmodules` is empty.
The actual [package metadata](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/pyproject.toml#L10)
(blob `e20a55506a9462acd76b7fadb5034722f263d878`) names
`https://github.com/commaai/opendbc`. `repos/ajouatom/opendbc` returned HTTP 404
with the available API access; no separate fork URL or revision can be established
from that result. It must not be invented from the owner's name.

The relevant carrot-wip history begins at the parentless
[`ecf4d58ce5df4d704fed1aeb942dd3454a388f52`](https://github.com/ajouatom/openpilot/commit/ecf4d58ce5df4d704fed1aeb942dd3454a388f52)
(`c4-v1`; committer 2026-01-27, author date 2023-09-27). The API reports
`parents=[]`. Local `git log --all -S` and exact-head GitHub path histories agree
that both disputed identities/mappings already appear at that root. There is no
accessible pre-root rename chain to infer from the older author timestamp.

### KIA: real orphaned mapping, rename intention unproved

At the root, [Hyundai values](https://github.com/ajouatom/openpilot/blob/ecf4d58ce5df4d704fed1aeb942dd3454a388f52/opendbc_repo/opendbc/car/hyundai/values.py#L493)
(blob `492eb67fa77fbaebb6d699b0cbcb44d778160f69`) already define
`KIA_K5_DL3_24_HEV`, while [torque params](https://github.com/ajouatom/openpilot/blob/ecf4d58ce5df4d704fed1aeb942dd3454a388f52/opendbc_repo/opendbc/car/torque_data/params.toml#L48)
(blob `9883acf2e5a853dbd7eeb18303ce90b189f80a7c`) already contain the different
key `KIA_K5_DL3_HEV_24 = [2.5, 2.5, 0.1]`. The old spelling is not an enum
definition in that values file. At the pinned head the same mismatch remains:
[values](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/hyundai/values.py#L509)
blob `f6c59cda30a25d6639a56fb83d708f428f7c7fa6`,
[params](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/torque_data/params.toml#L49)
blob `0630db145f2c3db453ea8ed1431d6d0a060dffc7`.

The complete values/params/override/substitute path histories back to the root
provide no old-to-new platform rename or substitution. Supplemental read-only
snapshots `carrot@62d004320d5a8683b919379f16811a832a894cf5` and
`c3-wip@f2118063e379fa7f5e0d1c0364910b79864ca553` also have the mismatch;
neither supplies a previously working old-key platform. The names and nearby
non-hybrid row suggest a key typo, but that is an inference, not proof of an
identical platform rename or of the tune's intended vehicle. **No alias, enum
rename or tune transfer is made; retain MissingTorque for this exact identity.**

### GMC: declared CC support, no exact CC tune

The root and pinned-head [GM values](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/gm/values.py#L222)
are byte-identical, blob `dfdbf11aaa832993a15d199d9cd2edaa480272b6`.
They declare `GMC_YUKON_CC` as "GMC Yukon No ACC", include it in
`CC_ONLY_CAR`/`CAMERA_ACC_CAR`, and simultaneously declare `GMC_YUKON`
as the ACC/LKAS model. Their specs differ: CC mass/wheelbase/steerRatio/front
ratio `2541/2.95/16.3/0.4`; ACC `2490/2.94/17.3/0.5`.
[Fingerprint source](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/gm/fingerprints.py#L163)
blob `ae10f80e76afb3c8ce5db8f06a3378134645bf4c` labels a 2017 Yukon,
and [interface source](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/gm/interface.py#L292)
blob `a3a8c401d6e5f3234013905dcbc1eade4253331c` explicitly calls
`configure_torque_tune(candidate, ...)` for CC. These establish declared fork
support, not functional parameter admission or measured tune equivalence.

The [override](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/torque_data/override.toml#L62)
blob `5fd55a39b7f505ed68a72d0eb6b5b746476ab479` has only the different
`GMC_YUKON = [1.2, 2.5, 0.26]`; the root override already did so (blob
`33868bca0ec214ee3b7ff3c7d315b89685a348fb`). The complete three torque-file
histories contain no exact CC tune/substitution, and the pinned substitute blob
is `c19ce46c71f110d24a5836f3b707b73905eaa3dc`. **This is a source omission,
not evidence that CC was deliberately unsupported. Retain MissingTorque; do not
copy the parallel ACC model's tune or remove CC from the catalog.**

### EPS_3: actual fork also preserves the unresolved conflict

Root and pinned-head [literal tests](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/can/tests/test_checksums.py#L49)
are blob `b06ab3262c4cae16dd39632c9165444d1a6bdf75`, still including
`7bf0026e`. Root and head [checksum implementation](https://github.com/ajouatom/openpilot/blob/7e371b92fd877c2cd160c92b4cf6fc62fe6d8805/opendbc_repo/opendbc/car/chrysler/chryslercan.py#L109)
are blob `76d05d6240ca1f916561e6b27afb0c78d81e3a1f`, still using
`0x122 -> crc ^ 0xF1`. The path histories contain only the parentless root;
the parser-snapshot test structure does not independently prove this sample was
accepted. No capture provenance or demonstrated algorithm correction was found.
Keep #181 open with its existing strict rejection and literal conflict unchanged.

Outcome: the two original-source-only tune unknowns remain unresolved after
checking the actual fork and its available history. No additional runtime change,
test/build execution, GitHub mutation or worktree/branch change occurred here.

## Additional cloud commands for the expanded scope

Run after the ordinary fresh native test/example build with the pinned source
oracle dependencies, retaining command status and all partial evidence:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-can --locked --test mlb_497 --test psa_asset_497
cargo test --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --test ford_override_497 --test ford_parameters --test psa_source
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_mlb_primary_497.py --binary rust/target/debug/examples/checksum_trace --evidence "$RUNNER_TEMP/vehicle-497/mlb-primary"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_psa_asset_497.py --binary rust/target/debug/examples/can_trace --evidence "$RUNNER_TEMP/vehicle-497/psa-asset"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_ford.py --binary rust/target/debug/examples/ford_trace --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-497/ford"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_psa_source.py --binary rust/target/debug/examples/psa_boundary --evidence "$RUNNER_TEMP/vehicle-497/psa-boundary"
```

The complete Card reference phase already includes Ford, factory, PSA boundaries,
CAN catalog/lifecycle and checksum comparisons; keep them rather than replacing
them with the focused recipes. The two new focused CAN tools and the regression
test names must be registered by the parent. Source controls/focused Card checks
use the existing `source_command` generated-DBC bootstrap when appropriate.
MLB primary comparison needs outbound access to the pinned raw.githubusercontent
URLs and authenticates bytes before execution. All these checks are pending;
no local test/fixture execution was used as a substitute.

## Owned changed paths (parent integrates CI and user docs)

- Preserved source: `opendbc_repo/opendbc/car/hyundai/carcontroller.py`,
  `tesla/carstate.py`, `volkswagen/{carstate,mqbcan}.py`,
  `torque_data/override.toml` under that car directory.
- PSA asset: `opendbc_repo/opendbc/dbc/psa_aee2010_r3.{dbc,provenance.json}`.
- CAN native: `rust/crates/can/src/{lib,volkswagen}.rs`,
  `tests/{native,mlb_497,psa_asset_497}.rs`, `tests/data/mlb_497.json`.
- Card native: `rust/crates/card/src/brands/hyundai/controller_buttons.rs`,
  `tesla/state.rs`, `volkswagen/{mod,state_mqb}.rs`, `psa.rs` under brands;
  `data/{vehicle,vehicle-497-provenance}.json`, `examples/psa_boundary.rs`,
  `examples/volkswagen_trace/steps.rs`, and
  `tests/{vehicle_backlog_497,ford_override_497,ford_parameters,tesla_params,registry,psa_source}.rs`.
- Exclusive tools under `rust/tools/`: `card_hyundai_alt_buttons_defect.py`,
  `check_vehicle_source_fixes_497.py`, `check_tesla_model_x_497.py`,
  `check_card_{volkswagen,tesla,factory,psa_source}.py`,
  `check_{can,can_checksums,mlb_primary_497,psa_asset_497}.py`, `can_cases.py`,
  `card_qa/volkswagen/{frames,scenarios,source}.py`,
  `card_qa/ford/scenarios.py`, `card_qa/tesla/scenarios.py`, `card_qa/psa/source.py`.
- This is the only vehicle evidence document added by this worker.

No encoderd/loggerd/model/GPS/torque-uploader/dev-tools/JPEG/server, shared
workflow/Cargo or paired user-guide files are owned by this worker. Other workers'
uncommitted changes in this shared worktree are preserved. #296 remains separate.

Earlier vehicle-batch static receipt (2026-10-23): 19 Python files parsed with `ast.parse`, four
JSON files and one TOML parsed; 16 scoped Rust files pass
`rustfmt +1.94.0 --edition 2021 --config skip_children=true --check`.
Scoped tracked diff and all 12 added-file whitespace checks pass. The owned set
has 42 paths. These are syntax/format/diff results only; functional, source-oracle,
native, ARM and vehicle results remain pending.
