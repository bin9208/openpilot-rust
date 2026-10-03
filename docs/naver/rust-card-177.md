# Native card and vehicle-interface conversion (#177)

Issue: https://github.com/bin9208/openpilot-rust/issues/177. Source baseline:
`31d7306882218e9fecc44aba0d5c034f0d1ca188`. Branch: `codex/feat-177-card`.
This is ongoing implementation of the approved full-runtime design. Native
startup and a continuous card CLI have been observed with owned original msgq
peers. All source brands now have native dispatch or an explicit inherited
failure boundary; the added six brands pass continuous host and ARM/QEMU IPC
comparisons. This is not a complete runtime or
device-test handoff, production selection change, or performance claim.

## Source scope and reuse

The actual registry contains 279 identities across 15 interfaces: body,
chrysler, ford, gm, honda, hyundai, mazda, mock, nissan, psa, rivian, subaru,
tesla, toyota and volkswagen. The reviewed #150 `control-policy` registry owns
controlsd's consumed acceleration/feedforward policies, not the vehicle CAN
decoders/controllers or native CarParams construction/startup queries.

The source inventory captures SHA-256, imports, declarations and line counts
for 242 Python files / 45,801 lines under card, opendbc/car and opendbc/can,
plus the Xiaoge payload module. It includes tests/tools and radar dependencies;
their presence in that inventory is not a claim that they belong to this card
assignment or have been converted. The state/controller/interface files and
shared car_helpers/fw_versions/interfaces/isotp/UDS files account for 11,157
source lines before card's Carrot cruise and other dependencies.

In this checkout CAN DBC parsing, packing and parsing are Python source. There
is no existing C++ CAN parser/packer boundary to retain. The new
`rust/crates/can` owns DBC metadata/definitions, big-endian/little-endian bit
layouts, signed values, source rounding, counters, all 13 checksum callbacks,
message frequency learning, registration grace, validity, raw accepted frames,
history, bus timeouts and lazy message registration in native Rust.
Generated Volkswagen CRC constants retain exact source provenance in
`rust/crates/can/provenance.json`; `rust/tools/can_source.py` regenerates them.
Python is an unchanged-source test/data-generation dependency only.

## Observed native component work

The native registry dispatches Body, Mock, Hyundai, Tesla, Mazda, Nissan,
Chrysler, Rivian, Ford, Subaru, Toyota, GM, Honda and Volkswagen: 278 catalog identities, 273 parameterizable
identities. KIA_K5_DL3_24_HEV, TESLA_MODEL_X, FORD_ESCAPE_MK4_5,
FORD_EXPEDITION_MK4 and GMC_YUKON_CC retain their original missing-torque failures.
Volkswagen adds 29 identities across PQ, MQB and MEB. PSA1 preserves its
typed source-failure boundaries and is not working CAN support. The shared
retained factory comparisons pass 123 fresh/repeated constructors, including all
parameterizable Ford, Subaru, Toyota and GM identities. Toyota SecOC startup
preparation forwards a valid saved 16-byte key to the controller before the
asynchronous CarParams cache writes.
The Honda and Volkswagen extensions add 26 and 33 factory cases respectively,
for 182 retained fresh/repeated constructor comparisons across these increments.

Honda adds all 22 source identities. Its sealed unit comparisons cover 1,056
parameter cases, 81 runtime profiles / 29,204 frames, 44 apply-before-update
failures and 32 numeric/Params failures, including complete partial histories.
Independent review found one startup error-order discrepancy when mandatory
powertrain definitions and the optional BSM DBC were both missing. The focused
correction validates powertrain definitions first, as the original constructor
does. A failing regression became green; all four asset tests passed. A retained
executable also reproduced the original review input's corrected typed error.

The Honda factory extension passes 26 fresh/repeated constructions through the
shared registry, including CR-V BSM and Accord CVT. Three focused source/native
runtime cases pass 1,440 frames (1,240 healthy and 200 invalid), covering both
CR-V BSM selections and the previously unexercised Accord GEARBOX_15T path.
The original sealed matrices remain unchanged and are reused for untouched
behavior; these new focused results are in the parent
`.omo/evidence/card-honda-parent-review/fixed/` record. Registry tests pass 3/3.
Honda Civic, Accord, Civic 2022 and CR-V 5G also pass real CLI IPC on host and
ARM/QEMU. The CR-V profile retains the original no-response ECU initialization:
ten exact UDS retry requests precede 320 normal warmup publications.

Volkswagen shared registration now passes the focused registry test that
previously failed with `UnsupportedVehicle`. The 23 MQB identities retain
the original first-update `np` NameError as a typed error; their seeded
controller tests do not establish normal runtime operation. PQ/MEB normal
profiles and continuous IPC are tracked separately. The shared factory gate
passes all 29 identities plus four repeated constructors, with exact original
stdout and fresh source DBC-cache behavior.
Independent review reproduced a MEB float32 speed boundary at 0.3 m/s that
changed ACC hold bits: the native comparison now promotes the schema value
to binary64 before comparison, matching Python. The reviewer and worker
retain both failing and corrected CAN/history captures for ID.4 Mk1/Mk2.
The independent Volkswagen review approves the port with these explicit source
limits, binding 12 stored lanes, 65 archived source files and seven executables.

The parent feature batch and focused SecOC regression pass 109 Card tests and
seven CAN tests. Initial
test-run failures were missing fixture environment and a missing copied Params
fixture; both failed commands are retained, and only pending targets were rerun.
All 130 Rust formatting changes outside Volkswagen are recorded with a preserved
pre-format archive and hashes; Cargo's workspace edition is 2021.

Continuous IPC comparison exposed a QA warmup error: wall-clock readiness alone
allowed different numbers of controller applications and therefore different
Toyota rolling counters. Fixed packet counts alone also failed when the native
receiver batched two packets after an 84 ms cold apply, and when the original
Honda ECU query consumed CAN packets during initialization. These failed trials
remain recorded. Warmup now waits for the previous complete runtime step before
the next input, and requires exactly 320 processed steps plus the explicit Honda
diagnostic prefix. Startup/warmup barriers observe the last real CAN packet.
The measured 80-frame stream retains an independent nominal sender deadline,
without catch-up bursts or receiver-driven pacing; its exact packet/timestamp,
validity, healthy-state and no-additional-timeout guards are unchanged.

The latest batch passes 17 scenarios / 1,360 measured steps per architecture:
Ford F150/Maverick, Subaru Ascent/Outback 2023/Forester preglobal,
Toyota Prius/RAV4 TSS2/RAV4 Prime, GM Volt/Bolt EUV, four Honda profiles and
Volkswagen Passat PQ/ID.4 Mk1/Mk2. Host uses normal policy at nominal 100 Hz;
ARM/QEMU uses the original `SIMULATION=1` branch at nominal 66.7 Hz CAN and
22.2 Hz controls. Full CarParams/CarState/prior actuator values, CAN bytes and
publication validity compare exactly. Every capture also observes startup
readiness, first empty-CAN error increment, SIGINT and durable Params drain.
Native process maps bind the retained host ELF or QEMU guest ELF, with no
Python library in the daemon. This is functional IPC evidence, not physical CAN
or target timing evidence.

Independent review found and reproduced a missing SecOC warning for valid hex
with an invalid decoded length. The correction emits the original warning
before subsequent route/CarParams writes. The expanded original-source oracle
passes all 354 cases, including 32 expected malformed-hex failures; four focused
Rust tests pass. The prior host executable and the corrected host/ARM executables
are retained separately. Existing IPC results remain bound to their actual
executables; the logging-only delta is covered by its own source comparison.

The corrected ARM CLI passes offline symbol/version resolution and loader
inspection against the five libraries extracted from the exact published
AGNOS 19.8 image (534 strong symbol references across six objects). No device
execution is inferred from this check. CI fixture generation and exact-SHA
integration are separate gates below.

The new CI paths generate fixtures in a fresh directory using pinned Python
dependencies and newly built original Params/msgq bindings. All eleven brand
input generators pass. The Hyundai fixture batch passes 18 assertions across
ten Rust integration targets, followed by full-schema comparison of 1,272
CarParams cases, 35,520 CarState records, 35,520 actuator ticks and 91,316 CAN
frames. The actual host CLI then passes nineteen source/native IPC scenarios
and 1,520 measured steps across all fourteen registered interfaces, including
Body active/passive modes and the CR-V diagnostic prefix. This run uses the
corrected CLI SHA-256
`a476f972e7fd634cabf70fbbe671459d7bd8313d01bc3cb0938bcfb36c3533e8`.
Its commands, generated inputs and complete captures are retained under the
Card worktree's `.omo/evidence/card-ci-reproducibility/`. This establishes the
local fresh dependency/fixture path; the complete required reference job,
exact-SHA GitHub Actions and post-merge checks still remain to be observed.

[PR #192](https://github.com/bin9208/openpilot-rust/pull/192) starts hosted
integration at `17db8ad83066561fdd69f45ee133b5abacff46cd`, after preserving
the integrated Bluetooth work from dev. The first
[fast check](https://github.com/bin9208/openpilot-rust/actions/runs/37114765269)
caught the unsupported job-level `runner.temp` import path and the inherited
required-job assertion missing Card. The repair uses the existing runtime
`GITHUB_ENV` import setup and extends the exact gate assertions to Card,
including failed, cancelled, skipped and absent results. All 22 CI-routing,
eight synchronization-guard and four Card CI tests pass locally. The failed
hosted run and corrected checks are retained; this is not a hosted pass claim.

The next hosted Card job built the native binaries, passed Clippy and its test
and Hyundai-schema steps, then stopped at the cruise source import: Python's
`-c` entry point put the unbuilt checkout msgq package before the runner binding.
The source bootstrap now uses Python 3.12's `-P` option, retaining the declared
PYTHONPATH order. A conflicting-working-directory regression fails before the
change and passes afterward; all five tooling tests and the import/CLI preflight
for all 32 reference checkers pass with the fresh binding environment. The
[failed job](https://github.com/bin9208/openpilot-rust/actions/runs/37115007935/job/111180009509)
retains its complete evidence artifact. No native runtime policy changed.

At `5023fb78`, the [PR Rust run](https://github.com/bin9208/openpilot-rust/actions/runs/37117329497)
passed every job, including the full Card source comparisons, nineteen actual
host IPC scenarios and ARM build. The independent
[push run](https://github.com/bin9208/openpilot-rust/actions/runs/37117327693)
failed the Mazda native warmup: all 320 ordered CAN packets were received, but
the receiver entered another unchanged 20 ms wait while the controller observed
phase completion. Its extra empty-CAN step appeared 20.804 ms after the final
receive. The failed archive is retained at SHA256
`b841b26d03ee06723911a1aaa794691b853116f542595756d531acb6b8c2edde`.
The passing PR run does not override the failed same-head push run.

The bounded fixture now explicitly arms a completed-step stop using
`--fixture-phase-fence`, accepted only with `--frequency-trace` and
`--max-steps`. Both the original-source diagnostic wrapper and Rust stop after
the monitor and flushed trace, before entering another CAN wait. Warmup setup
queues each next independently produced packet before resuming the receiver;
the fence is removed before the independently paced 80-packet measured stream.
Every one of the 320 setup steps retains its actual send/receive timestamp,
one-packet metadata, source diagnostic prefix, readiness and CAN error count.
No row is discarded and no runtime timeout or frequency/validity policy changes.

A controlled 100 ms final-observer delay reproduced extra empty steps on both
old source/native fixtures. A subsequent final-only-fence run exposed a distinct
31.588 ms mid-warmup producer gap. Its cause was not established from concurrent
host CPU activity. The completed-step setup seam was then tested with a deliberate
100 ms producer delay near tick 310 plus the final observer delay. Both lanes
retained exactly 320 setup and 80 measured packets, followed by the expected
post-stream timeout, SIGINT and Params drain. The delays and initial failures
remain in `.omo/evidence/card-phase-fence/`; stepped setup is not free-running
runtime or performance evidence. The final local matrix passes all nineteen
scenarios and 1,520 independently sent measured packets with native ELF SHA256
`431a47a2f61529c11dcaa4a5da55ba7ad2cc4617e1d5455a9b2fac73fc6c5884`.
Its `host-19/pumped/result.json`, command/exit records and per-lane audit retain
the exact inputs, 320 setup steps, CAN timing, validity and shutdown outcomes.
Seven affected Rust runtime tests, two Python fence tests, Clippy and formatting
pass; unchanged broad vehicle oracles were reused. Independent final review and
the parent's focused CI selection check also pass: the reviewer independently
recomputed all 19 measured pairs and 12,160 setup steps from retained data,
verified the frozen source/ELF identities and closed with no findings. Its
504-artifact receipt is SHA256
`2afe9179eda7f9b26cab195ffe9b44aa5cba8d71571db8100c59848fa28e578e`.
Exact-head hosted checks remain required after this repair.

Head `540612257ce27775d13fc81802371408da4e074a` then passed the push Rust run
[37123213229](https://github.com/bin9208/openpilot-rust/actions/runs/37123213229),
but PR run [37123214759](https://github.com/bin9208/openpilot-rust/actions/runs/37123214759)
failed the Nissan X-Trail exact comparison. Its preserved raw artifact SHA256 is
`5fa1aa6e355bc33eeda61137293259dbf056850fb2db01acd203c4693a8354b1`.
After consuming the last startup CAN, the original process continued through
two normal empty-CAN waits at 20.796 and 41.680 ms while the unfenced fixture
observed startup completion. This left counter 2 in every setup/measured state;
the native lane retained 0. Both lanes' 320 setup and 80 measured packets were
otherwise ordered and complete. The observed pause is a fixture boundary;
its host scheduling cause is not established. No error count is subtracted or
discarded to make the comparison pass.

The Python harness now arms the existing frame-0 fence before process launch,
observes its actual completed-step stop, and pauses the startup pump while the
receiver is already stopped. It resumes only to consume already queued startup
packets, one fenced step at a time, until the last sent timestamp is observed.
Every initial CP/CS/CO and nonempty CAN step must be present, with counter 0.
The original constructor, production Rust, native ELF and runtime timeouts are
unchanged. The setup fence is still removed before the independent stream.

The exact old helpers reproduce five startup empty-CAN steps in both lanes
under deliberate 100 ms observer delays before/after pause acknowledgment.
With the repaired helpers, both Nissan lanes retain two nonempty startup steps,
including the queued tail, and counter 0 under the same delays. A fresh full
matrix passes all 19 profiles, 1,520 measured steps and 12,160 setup steps across
both lanes; post-stream timeout increments, SIGINT and Params drain pass.
Seven focused tooling tests and Ruff pass. Evidence is retained under
`.omo/evidence/card-startup-fence/`; its receipt SHA256 is
`e8e927db02a8b6d232d6accfe6fe6ff23eabaf4e0b3016aa779740364abf4071`.
The two-helper source archive is bound to both successful captures; 454 prior
source files and the executed native ELF are unchanged. The single final
independent review passes with no actionable findings after recomputing all
19 pairs, controlled RED/GREEN, source/ELF mappings and shutdown outcomes.
Its 564-artifact receipt SHA256 is
`c1322172e398d7248294038342644a369d35bbcd7d008ac5249298b7321ba6db`.
New exact-head hosted checks remain required before merge.

A separate ARM replay passes 17 full vehicle traces / 7,700 frames across the
six added brands, with exact raw JSON equality to the retained source-equivalent
host results. Original Python oracles were reused without rerunning unchanged
cases. The candidate catalog exposes `openpilot-card`; its actual probe confirms
all four Onroad predicate combinations and preserves the source process entry.
The current Card/CAN source snapshot contains 358 files, archived before the
separate CI tooling changes. Independent review closes the SecOC finding and
passes the shared component gate; its receipt binds 476 evidence artifacts.

Native identification covers passive fingerprints, fixed/manual selections,
VIN, firmware matching/cache/query order, ISO-TP, ECU presence/disable and OBD
multiplexing. CarParams baseline/finish helpers preserve source failures.
Body/Mock source comparisons cover 3,400 full state/actuator/schema operations;
Hyundai's separate receipt records 1,272 full parameter cases, 35,520 complete
state and actuator schemas, 91,316 ordered CAN frames, and eight init/deinit
transcripts. Cruise's receipt covers 261 cases / 6,800 frames and 155 original
source tests. Tesla's separate receipt covers 137 scenarios / 4,960 frames,
including 50 actual CAN-timeout frames. Mazda covers 204 scenarios / 6,240
frames; Nissan covers 170 scenarios / 5,600 frames. Their complete schemas,
ordered CAN bytes, private state, Params effects and diagnostic queues match
their unchanged source. Python is a fixture/oracle dependency, never a daemon
runtime. Chrysler's sealed component receipt covers 669 cases / 9,600 frames,
including 214 ordered warnings and counter/checksum failure branches. A shared
counter-warning literal defect was reproduced and corrected without changing
parser decisions or CAN bytes.

VW's exact parser snapshots exposed a shared constructor-rounding discrepancy
at 33 Hz: the source divides one billion by frequency before multiplying by ten,
whereas the initial port divided ten billion directly. The constructor now
preserves the original operation order, agreeing at binary64 bits
`0x41b20fe01f07c1f1`; the old value differed by one ULP. A source-observed regression
failed before the correction and all seven focused CAN tests then passed.
Adaptive frequency recalculation already used the correct order. No tolerance
or source timeout policy was changed to conceal the mismatch.

Ford's owned component gate passes 207 cases / 4,680 runtime frames across all
nine parameterizable identities. It checks complete schemas, ordered CAN,
controller and parser state, warning/error logs, Params effects and lifecycle.
The two missing-torque identities fail explicitly. Two apply-before-update
failures and both infinite-pitch source `ValueError` boundaries are retained;
the latter initially completed incorrectly in Rust and now fail at the same
controller stage. A separate source-precision failure in `minEnableSpeed` was
corrected by performing the original double arithmetic before the schema cast.
Parent review also reproduced a NaN-acceleration partial-state mismatch:
Rust's `f64::max` changed the brake request before packing failed. The existing
source-compatible `maximum` helper now retains the source state; three pitch
profiles pass, with the failing captures and revision-two seal retained.
Three focused tests check startup error severity, emission before Params writes,
and propagation of a failing log sink. Ford source, binary and exact reports are
sealed under `.omo/evidence/card-ford/`; parent review remains required.

Subaru's separate component receipt passes 181 cases: 120 parameter cases,
31 runtime profiles / 11,160 frames and 30 pre-update failures across all
15 identities. Its receipt owns the ECU-disable and nonfinite-input boundaries.
Shared registration tests pass for Ford/Subaru, and the 54-constructor gate uses
source-built assets. Neither brand has a device or continuous IPC acceptance
claim from these component gates.

Toyota's separate component receipt covers 1,184 parameter cases, 77 runtime
profiles / 24,008 frames, 440 AES-CMAC vectors, 74 pre-update failures and 13
numeric/Params failures. GM's sealed receipt passes 676 cases: 496 parameters,
60 runtime profiles / 21,600 frames, 58 pre-update failures, two NaN failures
and 60 first-update pedal missing-message failures. It compares full schemas,
exact CAN bytes, private temporal state, logs, ordered Params writes and
lifecycle effects. The 123-constructor factory gate covers both brands and
Toyota's saved-key startup path. GM sources, binaries, RED/GREEN Params tests
and final reports are retained under `.omo/evidence/card-gm/`; Toyota's
component receipt is under `.omo/evidence/card-toyota/`. These are host
component/factory evidence, not new-brand continuous IPC or vehicle acceptance.

The shared core preserves source publication order: periodic carParams every
5,000 iterations, previous carOutput, carState, then init/apply/sendcan when
initialized and active. The final cruise projection follows enabled-edge
initialization. Modified CarState is committed back to the vehicle before apply.
`ControlsReady` follows interface initialization through the source asynchronous
Params queue; `FirmwareQueryDone` follows successful interface construction.
GM controller Params writes use that same queue before sendcan publication,
including writes queued before a later apply failure. Focused RED/GREEN tests
cover call order, enqueue failure propagation, unchanged no-write behavior and
shutdown drain; the queue retains the original unbounded FIFO contract.
The initial nonempty CAN packet and pandaStates precede identification. Native
CAN sockets retain non-conflated batches and radarInput CAN association.

The native CLI uses explicit checkout/numerical-artifact paths and original
service names. An owned full-original Car/interface/cruise/Params/msgq comparison
passed active Body, passive Body, Mock and model-bearing Genesis G70: 320
controlled steps, complete non-clock schemas and ordered CAN bytes. Each case
checks packet timestamps/counts, no additional controlled-step timeouts, both
publication validity outcomes, storage drain and SIGINT cleanup. An owned
SIGSTOP barrier separates startup publications from controlled inputs; host
clock fields and cumLagMs are excluded from cross-process equality. This is
host composition evidence, not physical CAN or vehicle validation.

The source core5/FIFO53 boundary passed all four Miri levels with injected
scheduler calls, PC bypass, affinity and first-error ordering. Real TICI
scheduling is not exercised. The native 100 ms Params reader, typed brand
warning/error sink and asynchronous writes have passed focused tests. Shared
core parity covers six scenarios / 5,912 steps. ARM/QEMU comparison covers six
scenarios / 480 controlled steps under the existing `SIMULATION=1` policy;
measured frequency observations distinguish that branch from normal-policy
ARM startup failures. It is not device timing validation.

Startup identification now compares 1,008 exact warning/severity/structured
event cases; unordered ECU response lists retain source set semantics.
Parameter diagnostics compare 1,676 cases. The Git banner helper passed
360 exact source URL cases after a retained bracket-suffix failure. A fresh
factory gate clears the original DBC cache and compares 28 fresh/repeated
constructors, including complete DBC/parameter/Common stdout for all nine
Chrysler identities. Source-generated Nissan and Chrysler DBCs are explicit
prepared assets in that owned gate.

### Build-time DBC packaging

`python3 rust/tools/card_prepare_assets.py REPOSITORY NEW_DBC_DIRECTORY`
prepares the DBC directory for a package at `opendbc_repo/opendbc/dbc`.
The destination must not exist and its parent must exist. It publishes only
after generation completes, rejects overwriting an existing package, and checks
the disk-space floor with estimated growth before staging. Python runs on the
build host only; card reads the resulting DBC files directly at runtime.

The current builder copies 65 existing DBC assets and runs the original pinned
generators in an isolated temporary directory for two Nissan, three Chrysler,
five Subaru and four Toyota assets (79 total). It preserves the existing Hyundai and Tesla
generated files and does not modify source generators. `card-assets.json`
records SHA-256 hashes for input sources and packaged DBCs. The configured GM
powertrain DBC is copied unchanged: its raw import declaration does not make
GAS_SENSOR/GAS_COMMAND available to the pinned parser. Pedal fingerprints retain
the source first-update missing-message failure; no invented expanded GM DBC is
packaged.

Three focused packaging tests passed: source-byte preservation and isolation,
refusal to overwrite an existing destination, and no partial publication when
required source input is missing. The factory checker now invokes the builder
instead of reusing prior evidence directories. The resulting Nissan/Chrysler
bundle passed all 28 fresh/repeated constructor comparisons; source captures,
binary hash and result are under `.omo/evidence/card-build-assets-factory/`.
The later additive Subaru bundle and manifest are retained under
`.omo/evidence/card-build-assets-subaru-dbc/`.

PSA is routed through its typed source-failure boundaries in the shared registry.
It has no constructible successful interface and does not increase working
brand/identity counts. The three PSA boundary tests and three registry tests
passed, including missing torque data and missing DBC propagation. Focused
Clippy completed with `-D warnings`; inherited native msgq compiler warnings
remain visible in `.omo/evidence/card-shared-clippy.log`.

A dedicated nominal100Hz publisher and actual ControlsReady/Hyundai parser
readiness barrier passed six host cases / 480 compared steps. A separate
ARM/QEMU `SIMULATION=1` corpus passed the same six cases at explicitly paced
66.7Hz CAN / 22.2Hz controls; normal100Hz QEMU batching remains a retained
failure. All 24 source/native processes verified first empty-CAN counter
increment, SIGINT and Params drain. Genesis reached counter201, both parsers
ready, and active ControlsReady1 before the measured stream. Initial
receiver-paced startup captures retain transient validity/initialization
differences rather than asserting synchronous equality. The native observer
initially reopened Params each tick and could block on a writer lock; the
corrected observer reads the existing handle. All 448 delegated receipt hashes
were checked directly.

Native carlog console forwarding/threshold/representation now compares 3,951
cases at each of ten valid LOGPRINT levels, including Unicode15 printability
boundaries and shortest-decimal ties. Four invalid levels fail explicitly on
both implementations; Python/native error transport text differs. Source
carlog's own console handler and cloudlog forwarding are both preserved.
The earlier eight-profile Mazda/Nissan/Chrysler/Rivian host and ARM IPC batch
is preserved under `.omo/evidence/card-ipc-pump-v2/`. Together with the original
six-profile batch and the latest 17-profile batch, the retained increments cover
31 scenarios / 2,480 measured steps per architecture across fourteen brands.
This aggregate spans separately hashed builds; it is not a single final-build
run. Remaining work includes reproducible CI, final receipt/integration gates,
full-runtime startup/upload composition and the explicit inherited source gaps. No
production daemon selection or first-device-test request has been made.

## Inherited source gaps

- [#189](https://github.com/bin9208/openpilot-rust/issues/189): all 23 MQB
  identities fail their first original CarState update because `np.mean` has
  no imported `np` alias. Original full CarInterface construction followed by
  `update([])` independently reproduces NameError for each identity. Four PQ
  and two MEB identities take separate paths. The native port must preserve
  the failure phase and preceding effects; seeded controller tests cannot
  establish working normal MQB operation. The original Python is unchanged.
- [#156](https://github.com/bin9208/openpilot-rust/issues/156): PSA CarParams
  construction lacks a torque-data key. Do not invent policy values.
- [#185](https://github.com/bin9208/openpilot-rust/issues/185): six catalog
  identities have no source torque entry: FORD_ESCAPE_MK4_5,
  FORD_EXPEDITION_MK4, GMC_YUKON_CC, KIA_K5_DL3_24_HEV, PSA_PEUGEOT_208
  and TESLA_MODEL_X. Native code retains typed failure without fallback values.
- [#186](https://github.com/bin9208/openpilot-rust/issues/186): inherited Hyundai
  alternate-button scalar behavior is retained; the native port does not silently
  change the source control outputs.
- [#188](https://github.com/bin9208/openpilot-rust/issues/188): inherited Tesla
  vehicle-bus button self-copy behavior remains unchanged.
- [#178](https://github.com/bin9208/openpilot-rust/issues/178): ten MLB checksum
  IDs call a three-argument XOR function with four arguments. Direct source
  calls raise TypeError; native returns an explicit typed equivalent failure.
- [#180](https://github.com/bin9208/openpilot-rust/issues/180): PSA selects
  `psa_aee2010_r3`, absent from both shipped files and source DBC generation.
  Source/native load failure remains explicit.
- [#181](https://github.com/bin9208/openpilot-rust/issues/181): public FCA EPS_3
  bytes `7bf0026e` have recorded checksum 0x6e while current source computes
  0xe1 and rejects the frame. Of 380 shipped literal FCA/MQB vectors, 379
  validate directly. This distinguishes a fixture/test gap from any assertion
  that the production CRC is wrong; authoritative provenance remains needed.

Source files, checksum rules, original test vectors and production daemon
selection are unchanged. No physical CAN, vehicle, private capture or keys
were used. Parent review and exact-SHA CI/integration remain later gates.

## Evidence receipt

Fresh evidence is in the issue177 worktree at
`.omo/evidence/card-177/2026-10-02/`; no ULW attempt exists in this worktree.
Source scope: `source-scope.json`. The retained
`resume-query/` results and command logs contain exact scenario scopes, source
hashes and executed binary hashes; its `binaries/` directory preserves executed
ELF files. Root manifest/report consolidation is still pending. Source/native
trace inputs/results and initial failures are preserved separately. Hyundai
and cruise have separate receipts under `.omo/evidence/card-hyundai/` and
`.omo/evidence/card-cruise/`. Tesla/Mazda/Nissan worktree ledger indexes link
their canonical root ledgers and pin the unchanged worktree artifacts. Fresh
handoff verification JSON files check their source hashes and retained ELF
bytes directly before registration. The latest shared checkpoint is
`resume-query/checkpoint-current.md`; pumped IPC QA has its own receipt under
`.omo/evidence/card-ipc-pump/`.
The parent six-brand integration batch, failed trials, current build receipts,
independent capture audits and SecOC correction are under the main checkout's
`.omo/evidence/card-fourteen-brands/`; `host-ipc-reconciled.json` and
`arm-ipc-reconciled.json` enumerate the exact selected captures and ELF identities.
Temporary generated DBCs and ARM runner wrappers are under the coordinated
root `.analysis/scratch/2026-10-02-rust-card/`.

The source catalog has 54 configured DBC names: 53 available, one missing PSA
asset. The comparison additionally uses the original test DBC and public
`fca_giorgio` DBC; those supplemental files are not additional supported
vehicle identities. Every codec byte, parser state/decision and metadata value
is compared; floating values use fixed absolute + relative 1e-12 tolerance.
Known source failures retain a failing observable, never a fabricated success.

Docs-Not-Needed: this internal native conversion does not change production
card selection, vehicle behavior, settings or user guide behavior.
