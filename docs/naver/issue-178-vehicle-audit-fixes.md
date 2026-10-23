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

#### afed hosted regression: lazy-parser fixture correction

Exact head `afed1a4638a8d0ec32b4f2744c47a80253748179`, Card job
`120045870465`, failed this regression at tick1 without a vehicle bus: actual
events were empty. The retained primary `card-afed.log` records all new PSA tests
passing before this Tesla test stopped the full suite. This is a fixture setup
failure, not evidence that the materialization repair should be reverted.

The original `CarState.get_can_parsers` constructs party/AP-party with empty
message lists. `CANParser.update` skips unregistered addresses, and only the
later `VLDict.__getitem__` registers `DAS_control` with zero signal defaults.
Native `State::new`/`update`/`signal_lazy` preserve that order. Thus the fixture's
first `DAS_accState=4` frame was not parsed. Its next zero followed last state
zero, not active3/4, so neither source nor native should emit Cancel. Changing
the runtime to consume that first frame would violate the original contract.

The positive fixture now performs an ordinary empty first update to register
the lazy messages, then sends real AP-party bus2 CAN for active3/4 followed by
each official cancel code0/1/2/12/13/14/15. It still requires the original exact
Cancel press at tick1 and release at tick5 after the four-sample pulse. With
vehicle bus enabled, real `UI_status2` bus1 touch0→3/3→0 adds LKAS press/release
after Cancel in the same list. Without that parser, only Cancel is emitted.
No pulse, event order, bus policy, parser validation or production code changes.

Separate actual-CAN negatives retain the cold first-frame loss explicitly,
reject wrong AP/UI buses, and distinguish empty CAN packets from no packet.
Independent touch edges during an existing Cancel pulse do not invent Cancel
edges; unconfigured vehicle-bus frames do not invent LKAS. Raw accepted DAS
payloads and complete accepted timestamp prefixes are checked at every step.
The source failure control uses the same ordinary registration setup for both
immutable baseline and maintained source, retaining input bytes, accepted raw
payload/timestamp receipts and all event rows. It still requires the baseline's
Unknown/false event loss with the vehicle bus and correct maintained values.

Parent Actions verification (already registered test/tool entry points):

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --test vehicle_backlog_497 tesla_
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_vehicle_source_fixes_497.py --output "$RUNNER_TEMP/vehicle-497/source-controls"
```

Use the existing staged-DBC `source_command` bootstrap for the Python call and
retain the unchanged full Card/Tesla source/native gates afterward. The 16×620
runtime rows, full state/actuator/CAN byte comparison and input-admission history
requirements are unchanged. No local functional execution, tests, imports,
builds, CI dispatch, commit or push; syntax/format/diff review only. This follow-up
owns the Tesla portions of the regression/tool and this section, no other lane.
Static receipt: Python AST parsing, pinned Rust1.94 formatting/check and scoped
three-file whitespace review pass. The original/native Tesla runtime files,
materialization correction and full comparison/scenario definitions have no diff.

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

## #180: exact PSA asset; runtime follows in #501

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

At the b37057897 checkpoint this was asset/codec work only: normal source
parameters lacked torque, source State lacked `parse_wheel_speeds`, and native
construction loaded the real asset then returned `RuntimeUnported`. The separate
#501 implementation below now addresses those runtime omissions. Asset acceptance,
hosted functional gates and physical CAN acceptance remain separate requirements.
Missing-file rejection is retained; no fabricated DBC or fallback was added.

## #185: exact angle admissions and inferred KIA key repair; GMC remains

[Primary overrides](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/torque_data/override.toml),
blob `f406ae64da4f2fa5a6b50e42b87feb2c7561d16b`, provide exact rows:
`PSA_PEUGEOT_208 = [nan, 2.0, nan]`, `TESLA_MODEL_X = [nan, 2.5, nan]`,
`FORD_ESCAPE_MK4_5 = [nan, 1.5, nan]`, `FORD_EXPEDITION_MK4 = [nan, 1.5, nan]`.
The **two Ford rows, Model X row and PSA row** are admitted to the source override and native catalog.
NaNs remain official placeholders (native JSON null), never measured friction or
LAT_ACCEL_FACTOR. Existing Ford brand parameter logic selects angle control;
source and native finish skip torque tuning even when NNFF is requested. Model X
and PSA retain their existing angle selection and dashcamOnly restrictions.

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

Keep #185 open for `GMC_YUKON_CC`, which has no exact key in primary override,
  [measured params](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/torque_data/params.toml)
  (blob `1fbabf6f76ba4bbc4717fb2e9797a4ef55616736`) or
  [substitutions](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/torque_data/substitute.toml)
  (blob `161c3792c4d3829e3bf3a6c4fda8c5dfe61742bb`). PSA admission is tracked in
  #501. KIA now uses the explicitly inferred unique-orphan key repair below;
  this is an inference from the existing fork row, not historical rename proof or
  a new calibration claim.

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

## PSA #501 implementation handoff; hosted functional gates pending

Dedicated follow-up: [#501](https://github.com/bin9208/openpilot-rust/issues/501),
with existing asset #180 and placeholder #185 acceptance retained. Parent committed
the Model X/Ford checkpoint as `b3705789764410d182e7bc9ac384634fa633e6cd`.
The follow-up now imports the exact
[primary speed helper](https://github.com/commaai/opendbc/blob/c3955753ae7258412814e2cdca3ee8af979e4679/opendbc/car/interfaces.py#L287)
(blob `34d557e5b535f3333ed1101a747c8d68f08a3ab1`): sum raw wheels, divide by four,
multiply by unit then wheelSpeedFactor, store cereal vEgoRaw, then pass the stored
value to the existing Kalman filter. No wheelSpeeds field or rounding policy is
invented. The actual helper has two body lines and is missing only from this
vendored base.

The implementation covers the one declared PSA identity, exact official
`[nan, 2.0, nan]` angle placeholder, existing source 3-bus state, shared wrapper,
20Hz steering controller/status 2→3→4 and full registry lifecycle/diagnostic
ownership. It preserves dashcamOnly, PSA safety, radarUnavailable, no longitudinal
CAN and all steering/rate/intervention limits. Source/native and actual IPC
comparisons, negative baseline/missing-asset controls and full hosted rig gates
are required before functional completion; physical CAN remains user-owned.
`psa.rs` remains the module entry, with `psa/{parameters,state,controller,runtime}.rs`
children and no `psa/mod.rs`. Registry dispatch covers parameters, construction,
controller/readiness, constructor DBC diagnostics, update, init, apply, commit,
soft-hold and log/Params ownership. The source's init/deinit are no-ops; the full
trace calls both. The pre-existing `Vehicle` trait has no deinit method, so no
unrelated global lifecycle API is introduced. `can_valid` diagnostics are drained
in the update that emits them, with parser order and short-circuiting retained.

Regression definitions use actual state/controller paths: ordinary empty first
update, then continuous 3-bus wheel/steering/cruise/body CAN, driver override,
2→3→4 activation, counter wrap, wrong-bus isolation and 20Hz signed-angle output.
The source/native oracle compares complete CP/CS/actuators, three complete parser
histories, speed-filter/owner state, packer counters, exact CAN, logs, constructor
Params and lifecycle. It defines 32 full-parameter cases, four NNFF×alpha-long
runs of 360 updates, inactive/active pre-update controls and a NaN-angle failure.
The actual IPC comparison admits PSA and Model X as passive dashcams, with NNFF
off and on and unchanged frequency/admission/shutdown gates. It does not expect
sendcan from a passive Card; direct controller comparisons establish output cadence.

`check_card_psa_source.py` authenticates primary Git blob
`34d557e5b535f3333ed1101a747c8d68f08a3ab1` and compares the original helper AST.
It loads immutable b370 interfaces/PSA state/torque tables to demonstrate the old
full-parameter KeyError and missing-method AttributeError, rather than injecting
a replacement helper into a control. Current controls use actual full parameters,
constructor and repeated updates with NNFF off/on, plus actual missing-DBC rejection
before constructor Params effects. Partial logs and pinned/baseline sources are
retained by the hosted recipes. Parent owns workflow/command registration and guides.

### Public registry behavioral RED/GREEN

`rust/crates/card/tests/psa_admission_501.rs` imports only existing public
registry/core/identification/cereal/Params APIs. Two tests require full exact PSA
parameter admission and construction from independently prebuilt angle CarParams,
then an actual first update/apply. No new PSA-specific API import can cause a
compile-only RED. It also checks existing readiness, constructor diagnostics,
commit, soft-hold and Params-drain APIs.

For hosted RED, prefer a clean b370 checkout and copy **only this test file** from
the proposed head. Run each exact filter separately with `--test psa_admission_501`:

- `public_registry_admits_exact_psa_dashcam_parameters`: old MissingTorque.
- `public_registry_constructs_first_update_and_apply_from_known_angle_params`:
  old RuntimeUnported after real DBC loading; the test verifies that error before
  deliberately failing. The proposed head must pass both unchanged tests.

If the parent's baseline helper instead restores production paths at the proposed
head, restore exactly these b370 paths:

```text
rust/crates/card/data/vehicle.json
rust/crates/card/src/brands/psa.rs
rust/crates/card/src/registry.rs
rust/crates/card/src/registry/diagnostics.rs
rust/crates/card/src/registry/vehicle.rs
```

The new child files become unreferenced by old `psa.rs`; there is no conflicting
`psa/mod.rs`. Build/run only this integration-test target on RED; new PSA trace
examples intentionally need the new runtime API. The isolated clean baseline is
preferred. Never replace production files in this shared worktree.

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
identical platform rename or a verified calibration. An independent review later
identified a unique orphan with exactly the same platform tokens in a different
order. The audit selected this narrow inferred key repair under the user's request:
`KIA_K5_DL3_24_HEV -> KIA_K5_DL3_HEV_24`, retaining the existing fork values
`[2.5, 2.5, 0.1]` exactly. It is not a transfer from another model, proven rename
history or measured-friction claim. Dewey owns the source substitution and Hyundai
regressions/source controls. This worker owns native catalog/provenance, public
registry admission and factory ordinary/second-Panda cases. KIA is removed from
the remaining MissingTorque list; GMC remains. Parent owns the stale generic KIA
negative assertion in `tests/vehicle_params.rs:14`; that file is untouched here.
Its other generic negatives contain no stale Model X/PSA rejection.

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

Outcome: the fork history does not prove a KIA rename; the selected inference
and retained exact row are explicit. GMC still lacks an exact supported tune,
and EPS_3 still lacks demonstrated algorithm/capture provenance. Those failures
remain strict. No local functional execution or GitHub mutation supplies missing
evidence.

## Additional cloud commands for the expanded scope

Run after the ordinary fresh native test/example build with the pinned source
oracle dependencies, retaining command status and all partial evidence:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-can --locked --test mlb_497 --test psa_asset_497
cargo test --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --test ford_override_497 --test ford_parameters --test psa_source
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_mlb_primary_497.py --binary rust/target/debug/examples/checksum_trace --evidence "$RUNNER_TEMP/vehicle-497/mlb-primary"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_psa_asset_497.py --binary rust/target/debug/examples/can_trace --evidence "$RUNNER_TEMP/vehicle-497/psa-asset"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_ford.py --binary rust/target/debug/examples/ford_trace --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-497/ford"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_psa_source.py --binary rust/target/debug/examples/psa_boundary --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-497/psa-controls"
```

The complete Card reference phase already includes Ford, factory, PSA boundaries,
CAN catalog/lifecycle and checksum comparisons; keep them rather than replacing
them with the focused recipes. The two new focused CAN tools and the regression
test names must be registered by the parent. Source controls/focused Card checks
use the existing `source_command` generated-DBC bootstrap when appropriate.
MLB primary comparison needs outbound access to the pinned raw.githubusercontent
URLs and authenticates bytes before execution. All these checks are pending;
no local test/fixture execution was used as a substitute.

## PSA #501 exact parent registration

These are registered Actions commands, never local execution. Next needed trusted
x64/AGNOS jobs follow the user's online `bin-actions` preference
`[self-hosted, Linux, X64, dev-md-local]`; actual ARM-CPU gates stay on GitHub ARM.
Do not cancel or repeat an unchanged running gate for this runner transition.
Parent owns workflow changes and resource-growth estimates.

After the existing pinned bindings/numerics and fresh DBC staging:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --test psa_source --test psa_admission_501 --test registry
cargo build --manifest-path rust/Cargo.toml -p openpilot-card --features native --locked --example psa_trace --example psa_boundary
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_psa.py --binary rust/target/debug/examples/psa_trace --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-501/psa"
PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/check_card_psa_source.py --binary rust/target/debug/examples/psa_boundary --numerics "$RUNNER_TEMP/card-native/numerics" --evidence "$RUNNER_TEMP/vehicle-501/psa-controls"
```

Use the existing `card_qa.ci.source_command(script, args, HYUNDAI_DBC_DIR)`
bootstrap for those Python calls to select the complete staged DBC root. The
primary helper control additionally needs outbound pinned raw.githubusercontent
access and the exact b370 Git object (fetch that SHA in the isolated job if needed).
The controls output directory must be fresh. Its optional `--binary` now compares
the complete runtime/Common path and therefore requires `--numerics`.

Parent's `check_card_ci.py` integration changes needed:

- `BRANDS` imported from `stage_card_corpus.py` now includes PSA, so the reference
  loop invokes `check_card_psa.py`/`psa_trace` automatically. Build both PSA examples.
- The later `check_card_psa_source.py` invocation must add `--numerics` and use
  `output / 'psa-controls'`, **not** `output / 'psa'`: the latter now belongs to the
  complete PSA reference loop. Preserve both invocations and partial artifacts.
- Stage fresh PSA and Tesla corpora. The standard all-brand corpus command now
  includes PSA and selects Model X in the Tesla corpus. Retained pre-Model-X
  Tesla input cannot satisfy that selection.
- Add actual IPC comparisons for PSA and Model X for **both NNFF settings**:

```sh
# Repeat for nnff=0 and nnff=1 with separate evidence roots, using the existing
# staged runtime-root from check_card_ci.py's IPC phase and fresh corpus.
python rust/tools/check_card_runtime_pumped.py --binary "$GITHUB_WORKSPACE/rust/target/debug/openpilot-card" --numerics "$RUNNER_TEMP/card-native/numerics" --binding "$CARD_PARAMS_BINDING" --runtime-root "$RUNNER_TEMP/card-native/ipc/runtime-root" --corpus-root "$RUNNER_TEMP/card-native/corpus" --candidate PSA_PEUGEOT_208 --candidate TESLA_MODEL_X --nnff 0 --evidence "$RUNNER_TEMP/vehicle-501/ipc-nnff-0"
```

The clean-baseline RED filters above must run separately from the complete new
runtime examples. Full Card/CAN tests, strict Clippy, source references, factory,
actual IPC and affected ARM gates remain required before completion. The current
worker reports only syntax/format/diff evidence. No physical CAN, new active
vehicle support, calibrated PSA friction or radar implementation is claimed.

## Owned changed paths (parent integrates CI and user docs)

- Preserved source: `opendbc_repo/opendbc/car/hyundai/carcontroller.py`,
  `tesla/carstate.py`, `volkswagen/{carstate,mqbcan}.py`,
  `torque_data/override.toml` under that car directory.
- PSA source base: `opendbc_repo/opendbc/car/interfaces.py`, exact primary helper only.
- PSA asset: `opendbc_repo/opendbc/dbc/psa_aee2010_r3.{dbc,provenance.json}`.
- CAN native: `rust/crates/can/src/{lib,volkswagen}.rs`,
  `tests/{native,mlb_497,psa_asset_497}.rs`, `tests/data/mlb_497.json`.
- Card native: `rust/crates/card/src/brands/hyundai/controller_buttons.rs`,
  `tesla/state.rs`, `volkswagen/{mod,state_mqb}.rs`, `psa.rs` under brands;
  `data/{vehicle,vehicle-497-provenance}.json`, `examples/psa_boundary.rs`,
  `examples/volkswagen_trace/steps.rs`, and
  `tests/{vehicle_backlog_497,ford_override_497,ford_parameters,tesla_params,registry,psa_source}.rs`.
- PSA follow-up native: `rust/crates/card/src/brands/psa/{parameters,state,controller,runtime}.rs`,
  `src/registry.rs`, `src/registry/{diagnostics,vehicle}.rs`,
  `examples/{psa_trace,psa_boundary}.rs`, `tests/psa_admission_501.rs`.
- Exclusive tools under `rust/tools/`: `card_hyundai_alt_buttons_defect.py`,
  `check_vehicle_source_fixes_497.py`, `check_tesla_model_x_497.py`,
  `check_card_{volkswagen,tesla,factory,psa_source}.py`,
  `check_{can,can_checksums,mlb_primary_497,psa_asset_497}.py`, `can_cases.py`,
  `card_qa/volkswagen/{frames,scenarios,source}.py`,
  `card_qa/ford/scenarios.py`, `card_qa/tesla/scenarios.py`, `card_qa/psa/source.py`.
- PSA/IPC follow-up tools: `rust/tools/check_card_psa.py`, `card_qa/psa/scenarios.py`,
  `stage_card_corpus.py`, `card_qa/runtime_{catalog,inputs,pumped_capture}.py`,
  `check_card_runtime_pumped.py`; the shared factory/source-fix tools include the
  PSA/KIA cases and updated remaining list.
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

PSA #501 follow-up static receipt (2026-10-23): 12 owned Python files parsed with
`ast.parse`, two JSON files and one TOML parsed; 13 owned Rust files pass Rust
1.94 formatting/parsing. Scoped whitespace review passes for 29 paths, including
eight added files. No project import, fixture/oracle execution, test, build,
commit, push or GitHub mutation was performed. `vehicle_params.rs` remains
parent-owned and untouched; its KIA line14 must become positive before the full
native suite. These results do not establish hosted functional or vehicle success.
