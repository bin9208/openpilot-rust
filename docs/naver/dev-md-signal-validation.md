# Final-source signal observer and planner integration

Source: `ajouatom/openpilot:carrot-wip` at `e8e152dbcfd9574a1791ed12026d1880e87689b3`, isolated `dev-md`, with `dev` fixed at `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. No device/NAS/deployment or promotion is authorized for this candidate. `rust/upstream/signal-source.json` holds exact Git blobs/content hashes for 54 source/test files. Source extraction tools are temporary differential references, not native runtime evidence.

## Behavior and ownership

Mode 0 disables the production worker. Mode 2 always uses one classical robust/verified housing tracker for preparation. Mode 1 uses the tracker unless the independent `neural_enabled` selection enables the optional CPU/QCOM/auto worker. Auto permits QCOM only with external driving GPU active and no internal DM need; switching uses a process boundary. Neural detector/classifier source identities are `1ad633066a1195006f4709f8fa07800dd65a74a814b3efb4c99bcc5a1a7962f6` and `455b71b3b20d3a96aa0e49f32714ba50421f668a2f9b9907c30b1346ac8a3703`. Native QCOM graph/kernel export remains open; a retained pickle is not a native artifact.

The old moving-red assist enable file is retired in production. Stopped guard, preparation and display use separate bounded observations. Unknown or old observations cannot release a latched stop. Preparation never creates acceleration permission or a measured stop line. Full planner ordering must be coasting, preparation, stopped-start guard, then final publication, with exact source context/driver gates.

## Current evidence (authoritative status)

All rows refer to the final pin above. Older entries below retain chronological
RED/repair history; they do not override this current matrix. None establishes
vehicle timing, normal candidate boot, or full optional-worker completion.

| Source paths / boundary | Native evidence | Remaining scope |
| --- | --- | --- |
| `openpilot/selfdrive/carrot/signal_{mode,assist_runtime,start_guard,stop_prepare,stop_prepare_runtime,road_policy,display}.py` | Guard/prepare/tracker full-state component corpora, 520 endpoint/cache reads and 576 fresh geometry-context cases match host/ARM; 25 constructor-cache cases/175 reads match host/ARM; planner 13 library tests pass host/ARM | Connected actual IO/full-owner62-frame faults match HOST and actual ARM source; active prepare3frames/62complete owner frames matches HOST. ARM active preparation and installed QCOM remain pending; source-file membership is not blanket coverage of retired analysis-only helpers |
| `carrot_functions.py`, final longitudinal/lateral owners and publisher | Actual source 360-frame active owner and every final longitudinalPlan wire field match host and actual ARM source, including exact Float32 fields, both triggers/all 14 fast reasons, events, validity and solve counts | Connected real subscriptions/endpoints/producer62frames also match HOST+actual ARM source. Diagnostic solver durations alone are captured metadata. ARM positive-prepare scene remains pending |
| `tools/signal_analysis/signal_tracker.py` + classical process | 288 complete causal frames, prior grayscale dimensions/hashes and 6,772 floating fields match host and actual ARM source; native real VisionIPC lifecycle passes host/ARM | Connected daemon/reader/full planner62frames pass HOST+actual ARM source; active prepare3frames HOST exact. Low-level OpenCV primitive discrepancies remain separately recorded |
| `tools/signal_analysis/neural_signal.py` CPU inference and interpretation | Eight real model calls/113,612 Float32 outputs exact per architecture; four complete RGB-to-inference cases exact host/ARM; 96 math cases, 168 tensor hashes, 1,633 host / 1,631 ARM proposals exact | Real CPU worker warm-up/active-model/VisionIPC/publication lifecycle passes HOST+ARM; final producer-to-planner acquisition and installed QCOM path remain pending |
| Same source causal neural sequence | 192 scripted-network frames/full private state exact host/ARM; 16 actual-inference active frames/16 tracks/full template hashes and 256 floating fields exact host/ARM, including gap/rollback | Synthetic images establish engineering equivalence, not signal applicability or vehicle acceptance |
| `signal_color_shadow.py::main/run`, `signal_neural_shadow.py::run_auto/automatic_backend`, production flag/backend readers | Manager latch32cases; auto70cases/2,628 actions; fresh Params480cases/1,540reads; file39cases exact independently HOST+ARM. Real native inert-child and typed-entry/classical-child lifecycle passes HOST+ARM, with missing-cache QCOM fault and no CPU fallback. Valid raw-mode prefixes1,009 match actual HOST Cython on HOST+ARM | Classical publication32cases/128reads and Loaded startup-mode race pass HOST+actual ARM source; Dispatch216cases/440reads and neural publication54cases/126reads match HOST+actual ARM source; connected classical producer/planner IO62frames passes HOST+actual ARM; installed QCOM caches remain open |
| NumPy 2.5.3 numeric primitives | Architecture-dispatched argsort: 1,920 cases/816,640 keys exact host/ARM. Float32 exp: 1,060,000 outputs bit-exact per architecture. ARM sort remains Rust portable; host x86 sort is authenticated external Intel BSD numeric code; x86 exp is scalar Rust source-compatible FMA arithmetic | Numerical output evidence excludes NumPy Python warning/floating-status machinery; no SIMD speed claim |
| `signal_neural_shadow.py::display_tracks` | 128 cases/1,720 outputs/10,320 floating fields exact host/ARM, including mixed labels, score boundary and 20-track cap | Real CPU worker and separate automatic child supervision pass HOST+ARM; installed QCOM path and phase-disable races remain pending |

Observer/numeric all-target strict Clippy passes with daemon/ORT features. The
isolated producer owns OpenCV/optional lazy ORT; neither library enters planner
or control process dependencies. Provider/model hashes and external licenses
remain explicit. No source-matched bundle freeze or device/NAS action occurred.

## Remaining gates

- Installed QCOM and fresh producer/planner IO remain open. Entry prerequisite/read order, neural record/start/display decode-error phases, classical publication rechecks and Loaded startup snapshot now pass HOST+ARM on the recorded corpora. Real CPU worker and automatic replacement remain scoped as described below.
- Genuine local QCOM graph/kernel/cache export and authenticated A630 artifacts, native provider/tooling and packaging. CPU fallback cannot substitute for this gate.
- Real producer -> planner subscriptions and bounded endpoints, fresh calibration/device/camera/livePose acquisition, mode/receipt races, and final publication through the actual owner. Component and callback-owner gates above remain scoped.
- Low-level OpenCV primitive discrepancy resolution: actual ARM has 25/13,284 template scores differing at most `1.0952353477478027e-6`, though complete causal source corpora match. Host source IPP/backend differences are separately recorded below; no tolerance or threshold was relaxed.
- Remaining project-owned developer/replay/export tooling conversion and one final source-matched candidate bundle at coordinated integration freeze. Normal boot/vehicle acceptance remain separate.

## Chronological implementation and validation record

The host OpenCV provider rebuild uses pinned external 4.13.0 source and cached archive, two CMake workers and an 8 GiB growth reserve. Initial build failed because SDK zlib headers were detected but absent from compiler search; retry uses explicit SDK C/C++ include paths. Primitive adapters only invoke external image operations and convert owned representations; observer policies remain Rust-owned. No Cargo is used outside the coordinated slot.

Native observer crate boundary agreed with root: `rust/crates/signal-observer`, eventual binary `openpilot-signalcolord`; OpenCV links only into this producer and tools, not planning/control loops. Root retains final manager/catalog enable predicate and packaging selection after verification. Initial RGB/current-lamp evidence helpers are being implemented; new APIs and image operations remain uncompiled until the coordinated next slot and completed host OpenCV provider. The model owner identified the existing native QCOM `QcomBundle`/`QcomModel` graph/weights/kernel boundary for optional neural work; signal-specific export, cache conversion and verified artifacts remain this scope's pending work.

## 2026-10-15 active state closure update

Final rebuilt host and ARM examples and all-targets plannerd clippy passed. Expanded stopped guard is GREEN for all 2,880 full decision/color/internal-state frames, including the captured empty-packet regression (31,692 floating state fields, zero bit differences). Preparation is GREEN for all 7,680 full decision/internal-state frames, including 3,767 active outputs and repeated-capture planner cadence cases (74,301 floating fields, zero bit differences). Actual-source image percentile/median comparisons are GREEN for 297 cases (594 float fields bit-exact). The four bounded native lib tests passed host and QEMU. These cover the state and statistics components, not camera pixels, fresh production subscriptions or final owner hooks.

Host external OpenCV provider is now sealed PASS at `/home/fma1/rust/cache/opencv-signal-host`, source revision `fe38fc608f6acb8b68953438a62305d8318f4fcd` and existing patch SHA `7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec`. Core/imgproc/dnn library SHA-256 values are respectively `c61087be017005d845247a21912718c150b983f241f7df645fd93b35d2fa3e10`, `f3ef57fc06dbd989e88ba6f10c54f522f45a0187faaab2ac1af2d81534128054`, and `582ea03b0b56b7cced59f65327a254d9c706682d011ef9803303c58cf0bf0151`. New primitives and pixel helpers await actual compiled comparisons; provider compilation alone is not that evidence.

Additional owner/context work written, pending focused validation: exact post-coasting preparation/start constraints (source owner-method corpus 3,840 frames), shared 0.5-second mode cache, production flag overrides, and current camera/calibration/livePose motion geometry checks. These are not yet connected to managed planner input/output. Production geometry accepts the exact OS04C10 front size/focal contract for tici/tizi/mici; other camera configurations remain invalid for this observer, as in source.

Optional neural asset preparation is local only: the exact source Autoware ONNX files (35,794,092/8,926,508 bytes) match the source detector/classifier SHA-256 identities. Official ONNX Runtime1.23.2 host/ARM archives match GitHub release digests `1fa4dcaef22f6f7d5cd81b28c2800414350c10116f5fdd46a2160082551c5f9b` / `7c63c73560ed76b1fac6cff8204ffe34fe180e70d6582b5332ec094810241e5c`; source commit `a83fc4d58cb48eb68890dd689f94f28288cf2278`. ARM ELF requires at most GLIBC2.27/GLIBCXX3.4.22. `rust/upstream/signal-runtime.json` records model URLs/revisions/hashes and full external runtime file hashes/licenses. This is verified asset preparation, not native CPU inference validation, target inference or packaging completion. Planned lazy provider loading preserves classical mode without mandatory ORT runtime loading. Native QCOM cache export remains separately pending.

## Native observer host comparison update

Bounded host verification passed OpenCV boundary 4 tests, observer transport 1 test, planner signal 6 tests and all-targets clippy for all three packages. Actual final-source lamp evidence is exact for 64 cases (320 floating fields); detector proposals/full detections are exact for 10 source-constructed road images (69 floats, 5 daytime detections). Post-coasting longitudinal constraints are exact for 3,840 frames (29,793 floats). Tracker update now compares complete internal state as well as outputs: 3,840 frames, 107,930 float fields, zero bit differences. Receipts are in planner scratch `pixels-native1`, `detector-native1`, `longitudinal-native1`, `tracker-state-native1`. These newest gates are host-only; producer process/owner integration and target execution remain pending.

The default host primitive oracle remains RED and is not silently relaxed. RGB gray/HSV, connected-component labels/statistics, opening, external contours and top-hat are exact. Template scores differ by maximum `2.7194619178771973e-6` (1,812 values exceed the original `1e-6` gate), with all 12 maximum locations identical. Distance transform differs by up to `6.079673767089844e-6` on ordinary masks; the deliberately all-foreground mask differs substantially (native 65533.8046875 versus source FLT_MAX). Investigation identified an external OpenCV backend difference: the Python wheel enables Intel IPP, while the sealed native build disables IPP. An explicit diagnostic disabling Python IPP makes every template score and distance value bit-exact. This establishes backend provenance only; it does not close the unmodified production-source gate or justify changing detector thresholds. `check_opencv_signal.py --disable-ipp-diagnostic` labels that diagnostic explicitly. Full causal-template decision/state coverage is still required.

Causal observer `Process` and capture policy/NV12 packing helpers are now written but disconnected and uncompiled pending the next coordinated slot. Actual-source `process-source1` contains 12 scenarios/288 road images, including translation, textured surround, gaps, rollback, missing current pixels and rejected green. Frozen source state contains 78 causal-template evidence instances, 132 confirmed-red and 35 confirmed-green track states, so this is active correspondence coverage rather than detector-only replay. Native comparison is still pending. Worker receive policy preserves the previous frame ID across tracker resets, matching source; copied NV12 is converted before caller rechecks the shared buffer frame ID. Scheduler/process entry and optional neural workers remain open.

All-thread scheduling is written in the isolated observer crate: calling-thread affinity intersected with CPUs0..3, fail if empty, each existing `/proc/self/task` thread set to that affinity then SCHED_OTHER/priority0/nice19, ignoring ESRCH only. One narrowly documented libc scheduling syscall is the unsafe boundary; policy and enumeration remain native Rust. This module is uncompiled and not called by a process entry yet. The observer adds only already-resolved libc/rustix dependencies; no dependency install or Cargo call was used during root's slot.

The signal source fixture now contains 54 exact Git blobs, adding final `openpilot/selfdrive/carrot/carrot_functions.py` and camera configuration `openpilot/common/transformations/camera.py` to the existing 52-file observer/runtime/test/tool closure. These are pinned source inputs, not covered native runtime claims. Publication source corpus `publication-source1` captures 128 complete start/prepare/display payload triplets, including neural score/age limits, missing optional neural fields, mixed color labels, source history exclusion and 20-track truncation; native comparison awaits the next slot. Corresponding `signal_publication_trace` uses the real native payload constructors.

Backend selection and child-stop lifecycle are written independently of inference loading: preparation mode always chooses classical tracking; an explicit CPU neural request selects CPU inference only in observe mode; qcom/auto requests use the source USB-GPU/DM ownership predicate. Missing `neural_backend` defaults to `cpu`; malformed values fail. A supervisor child must terminate and join within 2s, otherwise kill and join within 2s, before another backend can acquire GPU ownership. Actual-source `backend-source1` covers all 64 mode/device-policy combinations, awaiting native comparison. Native child supervision is not yet invoked by a process entry, and still requires process tests.

## Causal producer bounded gate update

Host causal image processing is GREEN for all 288 source frames: complete tracker update state/decisions/output, 6,772 floating fields with zero bit differences. This first corpus normalizes the private previous grayscale image to null on both sides; image history itself is additionally being extended to width/height/SHA-256 diagnostics, pending rebuild, so this result is not a full byte-history comparison. Host publication 128 triplets is exact (5,714 float fields), automatic backend 64 combinations exact, and all 12,257,280 RGB bytes from four padded/offset NV12 frames are exact. Source scheduling was observed in a native child: all four existing threads run CPU0..3/SCHED_OTHER/priority0/nice19. Native observer 3 tests and observer/plannerd all-target clippy passed after the style-only `is_multiple_of` fix.

ARM examples are linked using the sealed ARM OpenCV provider and an explicit per-example `rpath-link` to the existing AGNOS zlib directory; the initial link attempt lacked transitive target zlib and was repaired without changing runtime algorithms or comparison thresholds. Emulation is GREEN for publication/backend/NV12/pixel/detector and current longitudinal/tracker full-state gates, all values exact against the same pinned source corpus. ARM scheduling child includes the emulator helper thread: all five visible tasks have the required affinity/policy/nice. Full causal image sequence is still emulating; no completed target causal claim yet. These source-aware component results do not close the default IPP primitive RED described above.

A real classical road VisionIPC loop and `openpilot-signalcolord` entry are now written behind the explicit `daemon` feature, but UNCOMPILED and not selected by manager. The loop implements exclusive worker lock, all-thread scheduling, mode exits, reconnect/timeout/stale/rollback/session handling, post-conversion mapped frame-ID check, independent bounded atomic publications, structured logging and bounded cadence. Optional neural selection currently errors explicitly because its verified provider owner is still pending; this entry is not yet a full source worker or candidate startup claim. Fresh planner owner subscription integration remains pending. Root owns final manager/catalog and receives the final process contract only after daemon and producer/owner gates pass.

## Active final owner and production acquisition update

`active-owner-native3` runs the actual final pinned Carrot owner and both final MPC owners for 360 frames. It includes 172 start holds, 98 actuating preparation frames, 26 confirmed-green owner actions and 20 motorway-policy suspension frames. Full existing owner outputs/MPC trajectories/state plus start/prepare decisions and internal state and post-coasting constraint state have ZERO host differences. The exact native example SHA-256 is `16c729c0aabb854470b98e969afaeb2b4e5231aa09ccf969790493dfd6250829`. Geometry is an explicit callback fixture in this owner test, covered separately by the geometry oracle. This closes an active owner algorithm gap; it is not a production subscription or ARM owner gate. The first source capture exposed a missing injected longitudinal clock and is not used as equivalence evidence; the corrected source clock fixture produces the recorded exact result.

ARM causal processing completed: 6,772 floating state/output fields, 16 differing fields (eight scores duplicated in output and internal evidence), maximum `1.1920928955078125e-7`, all confined to current-image OpenCV `match_score`. Those score differences are at most one Float32 ULP. Every discrete decision, identity, color, streak, timing state and every other floating value matches in all 288 frames. The default exact comparison remains RED; no control threshold or generic comparison tolerance was changed. Detailed paths are in `process-native1/arm-differences.json`. This is a measured architecture/backend arithmetic difference on this corpus, not proof for untested near-threshold image correspondences. A newer snapshot includes previous grayscale dimensions and SHA-256; its host comparison is in progress and ARM rebuild remains pending.

The classical daemon entry and owner hook compiled: planner 10 host tests, observer 3 host tests, daemon all-targets check and host binary link passed; `openpilot-signalcolord --help` executed. Manager has not selected this producer. Since that compile, the owner hook was refactored to acquire inputs at the source phases (policy before params, start after stop filtering, preparation after final stop/mode), avoiding an earlier read becoming stale behind parameter I/O. Production acquisition adds `carrotNavi` as the eleventh main subscription while preserving the existing model/liveTracks poll. Calibration/camera/device subscriptions are lazy and separate, and recheck the five main receipts after their zero-time update and a new monotonic sample. Mode-change preparation reset, current pose angular context and structured source diagnostic events are written. These latest phase callbacks and production subscriptions are UNCOMPILED and unverified until the next bounded slot.

Additional source corpora now exist: `transport-source1` has 520 actual endpoint reads with complete cache/setting/vehicle context, including initial enable, later mode changes, identical bytes, malformed/non-object JSON, bounds, Python numeric/bool envelope behavior and missing timestamps; `context-source1` has 576 actual final preparation-runtime camera/calibration/livePose cases, 108 valid geometries and 24 turning cases, using the exact source camera configuration and rotation function. Native comparisons remain pending. A newly expanded stopped-guard case captured source boolean timestamp precedence (`malformed` native versus `stale_or_future` source); the Python-numeric-compatible fix is written and pending rebuild, with no departure permission added.

| Exact source group at `e8e152dbcfd9574a1791ed12026d1880e87689b3` | Native ownership and evidence | Remaining closure |
| --- | --- | --- |
| `carrot/carrot_functions.py`, final longitudinal owner methods | `plannerd/carrot/{signal,update,traffic,navigation}.rs`, `signal/longitudinal.rs`; active full owners plus every plan wire field 360 host/actual-ARM-source frames exact | Real producer/subscription mode/receipt/race cases |
| `carrot/signal_start_guard.py`, `signal_stop_prepare.py` | `plannerd/signal/{start,prepare,priority,geometry,statistics}.rs`; expanded bool-timestamp guard 2,970 host/ARM frames and prior preparation full state/geometry exact | Actual subscription mode changes and producer/owner corpus |
| `carrot/signal_assist_runtime.py`, `signal_stop_prepare_runtime.py`, `signal_mode.py`, `signal_road_policy.py`, camera configuration | `plannerd/signal/{transport,context,road_policy}.rs`, `runtime/signals.rs`; 520 transport/576 context cases exact host/ARM; phase hooks compile/clippy | Lazy actual subscription/cached-constructor mode/receipt race tests |
| `modeld/signal_tracking_shadow.py`, `tools/signal_analysis/signal_tracker.py` | isolated observer detector/pixels/process/worker/runtime; 288 causal complete pixel/history/state frames exact host/actual-ARM-source; real VisionIPC lifecycle host/ARM green | Producer-to-owner pipeline; low-level primitive numerical limitation retained |
| `modeld/signal_color_shadow.py`, automatic selection in `signal_neural_shadow.py` | observer scheduling/backend; host/target thread properties and 64 selection cases exact; actual TERM/KILL join and mode-exit green | Automatic backend switch/ownership and complete dispatch |
| Neural inference and sequence/export tools | actual host/ARM CPU ORT 8 runs 113,612 outputs exact; temporal sequence 192 frames exact; portable sort 816,640 keys exact | End-to-end preprocessing/postprocessing, x86 sort boundary, active CPU/QCOM worker/export, packaging and native developer tooling |

Source fixture membership and source-only corpora are not coverage completion. Full producer/owner/runtime source closure and final source-matched package remain pending.

## Latest bounded production-phase and ARM evidence

The phase callbacks and actual fresh-subscription implementation described above now compile and pass plannerd's 11 host tests and all-target clippy with warnings denied. The observer daemon feature also passes all-target clippy. Re-running the 360-frame active final owner after this refactor still gives zero host differences. Actual pinned-source endpoint/cache comparison is exact on host and ARM for 520 reads (2,060 floating fields); camera/calibration/pose context is exact for 576 cases (432 floating fields). The expanded boolean-timestamp stopped guard is exact for 2,970 frames (32,772 floating fields) on both architectures. The ARM observer library harness passes all three tests and the linked native ARM daemon executes `--help`.

The active full owner executes on ARM, but its exact floating comparison remains RED: 120,546 of 339,346 floating fields differ, maximum `4.023970046063141e-11`. Every discrete/state/identity/validity decision matches across all 360 frames. Maximum differences by group are longitudinal/MPC `4.023970046063141e-11`, lateral `2.581934666068264e-12`, Carrot `3.3173463975799677e-13`, and signal constraint cap/previous `5.750289133743536e-12`. Guard and preparation decisions/internal state match. A passive ARM solver interceptor reports `[362,360,0,0]` (lateral calls, longitudinal calls, comfort retries, other retries), while leaving native ARM results identical. Converting candidate longitudinal outputs to Float32 exposes 15 differences in near-zero `j_target`; actual wire packet closure is still pending. These measurements do not establish target bit equivalence or justify changing a behavior gate.

The causal image sequence now also checks the private previous grayscale image dimensions and SHA-256. All 288 host frames, complete history hashes and 6,772 floating fields match exactly. The corresponding updated ARM run is pending; the earlier 16 one-ULP match-score differences remain explicit above. A real isolated VisionIPC daemon lifecycle example is written but not compiled: stale input, duplicate suppression, rollback/session reset, exclusive worker lock, three independent publications, and TERM-to-KILL joined-child escalation. It is a process correctness gate, not vehicle timing evidence. Real producer-to-planner fresh receipt/mode/race/wire coverage, optional inference/export and the final source-matched package remain open.

Updated ARM causal byte-history gate completed: all 288 previous-image dimension/SHA-256 snapshots and all discrete state match exactly. The same 16 match-score fields differ, maximum `1.1920928955078125e-7`; no new floating differences were introduced by the full history comparison (`process-pixel-history1/receipt-arm.json`).

Further native neural sequence work is explicitly pending: the actual final `NeuralSignalSequence` source has now generated eight scripted-network scenarios/192 frames with full template/mask image hashes, complete internal state and network call ordering. It covers translation, missing current pixels, detection loss/reacquisition, timestamp gaps/rollback/repetition, ambiguous proposal association, flat surrounds and the 20-object cap. `signal-observer/neural_sequence.rs` and its trace are written but uncompiled. This isolates temporal policy for testing; scripted network results do not establish actual ONNX inference, QCOM execution, artifact conversion or a complete optional worker.

## Real producer lifecycle and neural temporal policy gate

`signal_runtime_trace` now passes on both native host and linked ARM under QEMU, using real isolated road VisionIPC, the production classical loop and actual child processes. It exercises stale capture rejection, duplicate suppression, rollback/session reset, exclusive lock exclusion, three separate bounded publications, mode-change exit, and a child that ignores TERM and must be killed and joined after the source 2s grace period. It uses no production endpoints or vehicle connection and establishes process correctness rather than target timing/performance. Host four OpenCV, seven radard and three observer library tests pass; observer daemon-feature and radard all-target clippy pass with warnings denied. The first OpenCV test invocation lacked the existing required `OPENCV_TEST_LANE_MODEL` environment input; supplying the exact retained lane model makes all four pass without implementation changes.

The actual final neural sequence comparison is now GREEN on both host and ARM: 192 frames, 61 detector calls, 645 classifier calls, 474 tracked outputs, complete internal state and template/mask SHA-256 values, and 11,088 floating fields with zero differences. The source 20-track limit is exercised. The native implementation uses the source `TM_CCORR_NORMED` primitive; existing classical `TM_CCOEFF_NORMED` behavior is preserved. These network results are scripted: this closes temporal association/template/expiry/reacquisition policy on the corpus, not ONNX or QCOM model execution. `neural-sequence-source1/{receipt,receipt-arm}.json` retain exact comparisons. CPU ORT lazy loading and execution, QCOM graph export/provider ownership and real producer-to-owner fresh mode/receipt/wire cases remain open.

## Actual CPU inference reference and lazy ABI boundary in progress

The final `NeuralSignal` CPU implementation has executed the pinned detector/classifier ONNX files on four current RGB images with ORT1.23.2, explicit CPUExecutionProvider, one intra/inter-op thread and no intra-op spinning. Eight source inference runs produce two detector proposals and classifier labels unknown/red/green/green. `neural-inference-source1/expected.json` binds all input/output Float32 bytes, shapes and hashes plus reference Python-wheel provider SHA-256 `0d965a8252784479570654b909e99f94955491450394f013ac391e11a30828de`. This is a temporary actual-source differential reference; no native inference comparison has run yet.

A new isolated `onnx-runtime` crate is prepared outside workspace registration and remains uncompiled/unreferenced. It validates pinned provider/model identities, opens the exact versioned ORT ELF lazily, and confines the external C ABI calls to a thin CXX adapter with owned results and synchronous borrowed input. Only C declarations are compiled; there is no planned ORT `DT_NEEDED` entry or load in classical mode. CPU preprocessing/postprocessing, exact confidence/sort/box boundaries, real inference output comparisons, active worker dispatch, QCOM graph/cache conversion and packaging all remain pending. These new files are not a verified runtime provider until the next owned compile and actual model gates succeed.

A separate actual final preprocessing/postprocessing corpus is now generated (`neural-math-source1`): 96 cases, 1,631 retained proposals and 168 captured tensor hashes. It covers source confidence boundary products, equal-score ordering, 17/32/65-row candidate sets, clipping/large boxes, full and cropped detector inputs, integer classifier cropping, empty crops, label argmax ties and resize/normalization. Native comparison is pending. In particular, NumPy's unstable mixed-score argsort tie order and Float32 exponential/box arithmetic must be preserved or measured explicitly; a generic stable-sort substitution is not accepted as source equivalence.

## Exact same-architecture full owner closure

The final full active owner now passes against actual ARM source execution, with no numerical relaxation. ARM CPython3.12.3 extracted from the verified cached AGNOS image, official ARM NumPy2.5.3, pycapnp2.1.0 and the actual source MPC Cython wrappers execute the exact pinned final Carrot and both final MPC owners for 360 frames. Native ARM outputs, trajectories, complete internal state, active start/preparation decisions and post-coasting constraint state have ZERO mismatches, including floating bits. Coverage remains 172 holds, 98 actuating preparations, 26 green actions and 20 policy suspensions. Native ARM example SHA-256 is `1f299dc88b93344044b58a4db6864e4adffea0f75350f44bab2e62972cc4a9e5`; receipt is `active-owner-arm-source2/receipt.json`.

The source MPC wrapper build uses the existing verified ARM acados artifact, locked Cython3.3.0 generator and actual ARM NumPy headers. SDK Python3.12.3 headers required the missing target configuration header; it was obtained from official Ubuntu Ports `libpython3.12-dev_3.12.3-1ubuntu0.17_arm64.deb` (SHA-256 `945ad3f651f683c11e72d19d804b1cff097308dcffa01057635cba038f875164`), whose target pyconfig SHA-256 is `c0dc659a80f55ab7b441abf018a9d05e8961bbb2d003e1c8757570b92e21f5ca`. The header is isolated under scratch and does not alter the shared SDK. Source build/identity receipts are retained in `source-final-arm3`. Only test/reference packages were added; production remains native.

The previous host-reference-versus-ARM rounding measurements remain valid architecture comparisons, but they do not represent a target source mismatch: the same-architecture source gate has none of those near-zero jerk crossings. This result closes the active target owner arithmetic gap on the corpus. Explicit geometry callbacks remain fixture inputs with separate actual-source geometry coverage; real producer-to-owner acquisition races, mode changes and publication packet comparison are still separate open gates.

Actual ARM Python OpenCV4.13.0.92 source primitives were also captured without disabling any source backend. All gray/HSV/labels/statistics/opening/contours/top-hat and all 18,432 distance values match the sealed native ARM provider, resolving the earlier IPP all-foreground distance difference for target execution. Template scores remain a measured open backend discrepancy: 25 of 13,284 score fields differ, maximum `1.0952353477478027e-6`; no primitive/behavior tolerance was changed. `opencv-actual-arm-source1/actual-arm-source-receipt.json` records exact counts.

## Native lazy CPU provider host gate

The isolated `openpilot-onnx-runtime` member is now registered and compile-coherent. Host native execution against the actual-source CPU tensor corpus passes all eight detector/classifier runs: 113,612 Float32 output fields are byte-exact. Strict all-target clippy passes. The ARM tensor example is linked and target execution is pending. `readelf` lists only standard C++/GCC/libc/interpreter dependencies, confirming no mandatory ORT `DT_NEEDED`; no observer or control component depends on this crate yet. `ort-native-host1-receipt.json` records every run's full output hashes and zero differences. The reference ORT Python package remains confined to ignored test scratch.

Actual ARM source inference has independently run all eight model calls. All eight preprocessing tensor byte hashes match the host reference. ARM source output arithmetic differs from host in every run as expected for an architecture comparison; native ARM versus actual ARM source output comparison is still running. This is not yet an active optional worker or complete CPU neural behavior gate.

## Actual target causal pixels and CPU inference closure

Actual ARM source processing of the identical frozen RGB inputs now matches native ARM for all 288 causal observer frames, complete internal state/output, previous grayscale dimensions/SHA-256 and all 6,772 floating fields: ZERO differences. `process-actual-arm-source1/native-comparison.json` records this result. The earlier sixteen one-ULP match-score differences remain valid host-reference architecture comparisons; they are absent against the target source. The primitive score discrepancy described above still exists for that separate low-level corpus and is not silently hidden by this application result.

The native ARM lazy CPU provider likewise matches actual ARM source ORT inference for all eight detector/classifier runs and all 113,612 Float32 outputs, byte for byte. The actual ARM reference pybind provider SHA-256 is `85ed5b80d6cb32a2e834727f84031929e831843870babad928b9c1b93a5467bf`. All eight source preprocessing input hashes match the host capture, so the frozen source input tensors are identical. Per-run exact counts/hashes are retained in `ort-native-arm-actual-source1-receipt.json`. Thus both host and target have actual-source model execution evidence; native end-to-end preprocessing/postprocessing and active neural worker/supervisor/QCOM export/provider still remain open.

NumPy's exact2.5.3 source sdist was retrieved from official PyPI and matches SHA-256 `df2d5874ff183595a4ba404edd04f6bd9b5505c1d7708573f6a6c17489a67563`; the advertised GitHub revision/tag were unavailable through raw GitHub. Source `npysort/quicksort.hpp` SHA-256 is `ee55b06cdc82556f5bac021a605419561203def7fd4074b5039c5f9ca665801e`, and `npysort_heapsort.h` is `4c3f0b0a572431975c4a9f0849c746e30441b3912ff2f8c64680673078df25b1`. Its ARM argsort path is portable introsort/heapsort; only x86 dispatches SIMD argsort. The Rust portable path is written with the complete NumPy BSD notice retained in `signal-observer/NUMPY_LICENSE`, pending native verification. The host SIMD tie-order implementation remains a distinct requirement. Actual target96-case math output differs substantially from host under mixed score ties, so architecture-specific source references must remain explicit.

## Real final publication regression and exact wire fields

The active full owner oracle now also invokes actual final `LongitudinalPlanner.publish`; the native example invokes the real Rust publication writer and decodes every field through the schema. The first run captured a real production defect at frame3: native publication rejected source `audioRefuse` emitted by the new signal paths. The writer now accepts that event and retains the actual source `warning=true` flags. The follow-up absent `gpsTrajectory` mismatch was confined to the new test decoder's null-pointer handling and required no production change.

Host and actual ARM source comparisons both pass all 360 complete owner frames plus EVERY longitudinal-plan schema field with zero differences, including promoted Float32 bits, events, validity, timestamps, both planning triggers, all fourteen fast lead reasons and varied fast metadata. Solver/planner/fast execution durations are explicit zero fixture metadata in both references, so this gate is not a timing/performance measurement. Source contexts and geometry callbacks remain explicit inputs as documented previously. Receipts are `active-owner-wire-{host,arm}-source1/native-wire-receipt.json`. Planner11 library tests and all-target strict clippy pass; fresh ARM owner example links and executes successfully.

The portable NumPy sort is now integrated and verified by Cargo host/ARM examples: 1,920 actual ARM-source cases /816,640 keys are exact, including empty/small/large arrays, unstable ties, NaNs, infinities, signed zeros, ascending/descending and organ-pipe order. Source primitive and native comparator retain NaN placement. Native host and ARM use the same Rust portable function in this component gate; this is target portable order, not x86 SIMD-order equivalence. Strict observer no-default-feature clippy passes. Separately, a standalone source-exact external NumPy x86-simd-sort v4 numerical primitive matches the actual host NumPy oracle for all 1,920 cases /816,640 keys. Its checked optional host-only production boundary/dispatch remains pending; no NumPy Python runtime is introduced by the Rust portable path.

### Native neural pixel and output interpretation (2026-10-15)

`signal-observer::neural` now owns detector BGR crop/416 letterbox, classifier
224 RGB normalization, grid/Float32 box decoding, confidence filtering,
NumPy-compatible ARM sort, NMS, and classifier labels. The actual pinned ARM
Python source and native ARM example match exactly for 96 cases, 1,631 retained
proposals, 168 complete preprocessing tensor hashes, and every classifier result.
Receipt: `.analysis/scratch/2026-10-15-dev-md-planner/neural-math-actual-arm-source1/native-receipt.json`.
These use scripted logits; this gate does not replace the separate actual-model
inference gate. Host preprocessing hashes and labels also match all 96 cases;
host proposal parity was subsequently closed by the source x86 sort/exp dispatch below. Native
host/ARM examples link and the observer default all-target strict Clippy gate
passes. The optional ORT adapter/full pipeline example now links and runs on host and
ARM. Four real frozen images execute both model sessions (eight calls) and match
each architecture's independently executed source: two proposals, four classifier
results, and all 14 interpreted Float64 fields are bit-exact. This complements
the earlier 113,612 exact Float32 raw model outputs. No neural producer is
selected by this result.

### Constructor cache and full native CPU interpretation gate

The production planner now seeds its shared mode cache at the initial parameter
read, retaining the source's half-second constructor cache before the first
planning phase. `RuntimeSignals::new_at` keeps the time injectable. Actual source
25-case/175-read fixtures match both host and ARM, including setting changes
before the first phase and at the exact expiry boundary. All 13 planner library
tests pass on host and ARM/QEMU, including the injected runtime constructor
regression. Planner and observer all-target strict Clippy passes with daemon and
neural-ORT features. Receipts: `mode-constructor-source2/{host,arm}-receipt.json`
under the planner scratch directory.

Full CPU interpretation receipts are
`neural-inference-source1/fullpipeline-receipt.json` and
`neural-inference-actual-arm-source1/fullpipeline-receipt.json`. These exercise
native RGB preprocessing, authenticated ORT sessions, and native output policy;
real daemon acquisition and backend supervision remain separate pending gates.
The first 16-frame actual-inference temporal corpus produced no tracks and is
explicitly insufficient for active temporal coverage. A textured synthetic ROI
recognized by the unchanged source detector is being used to extend it; no
threshold or validity relaxation is authorized by this result.

### Architecture-dispatched math and actual active neural sequence closure

The checked optional `signal-numerics` crate compiles unchanged Intel BSD-3-Clause
NumPy-vendored x86 argsort headers for v3/v4, verifies all source hashes and checks
the complete ISA groups before calling a borrowed-slice CXX adapter. ARM compiles
no x86 numeric object and retains Rust portable sorting. Source-license and full
identity metadata are in `signal-numerics/{PROVENANCE.md,vendor/source.json}`.
`numpy_exp` retains ARM libm and reproduces each source x86 SIMD arithmetic lane
using scalar Rust FMA operations; this is numerical compatibility, not a speedup.

Receipts under the planner scratch directory:
`numpy-sort-actual-{host,arm}-source1/native-dispatch-receipt.json`,
`numpy-exp-native-{host,arm}-source1/receipt.json`,
`neural-math-source2/native-dispatch-receipt.json`,
`neural-math-actual-arm-source1/native-dispatch-receipt.json`,
`neural-display-source1/{host,arm}-receipt.json`, and
`neural-real-sequence-active-{host,arm}-source1/{native-dispatch-receipt,native-receipt}.json`
(the host uses the first filename, ARM the second). The actual-inference temporal
corpus uses a frozen textured synthetic ROI recognized by the unchanged source
detector, yielding 16 real tracked outputs and full template histories through
gap/rollback. The earlier sparse 16-frame no-track corpus remains explicitly
inactive coverage. Both architectures have zero differences, including floating
bits; source references run independently on each architecture.

### Final manager-entry latch

`signal_color_shadow.main` catches ordinary worker failures and then waits while
signals remain enabled. It also waits after a normal worker return, including a
mode change; a manager restart must not silently bypass that source latch.
Native main now uses `lifecycle::finish` with ERROR/exception logging. Bounded
CLI invocations return directly, matching the source direct-run CLI distinction.
Actual final-source 32-case action sequences match host and ARM exactly for
successful returns, failures, all requested checks and one-second waits. Receipts
are `worker-latch-source1/{host,arm}-receipt.json` in planner scratch. Host and
ARM daemon binaries link; ARM `--help` executes; observer all-target strict Clippy
passes with daemon/ORT features. An initial host example launch omitted the
required OpenCV library path; rerunning with the verified provider path passes.
This proves the helper/main integration and finite source actions, not a real
neural worker lifecycle or complete manager/producer ownership.


### Real CPU neural worker and pending classical lifecycle closure

The isolated native `neural_runtime::run_cpu` now passes the real VisionIPC
fixture on HOST and actual ARM/QEMU. Authenticated source models execute warm-up
before subscription, then process a padded NV12 fixture with active source
tracks. Independent original-source references match the complete RGB hash and
publication payload floating bits on each architecture. Two frames/publications
cover stale capture consumption, duplicate rejection, rollback/session reset,
exclusive worker locking and disable exit; no prepare or control permission is
published. Logs are `neural-runtime-{host,arm}-live1.log` and references are
`neural-runtime-input-{host,arm}-source1/expected.json` in planner scratch.
The source warm-up loop also matches six calls, shapes and complete input hashes
on both architectures (`neural-warmup-source1/{host,arm}-receipt.json`).
This extends the earlier helper-only lifecycle evidence to a real CPU worker;
main selection, automatic child replacement, genuine QCOM ownership and fresh
planner subscription acquisition remain pending.

Final classical worker audit additionally found the independent comparison-file
change exit and one-second atomic `tracking_latest.json` snapshot. Native code
and its real IPC regression now include both: HOST and actual ARM/QEMU pass
comparison-file exit, complete latest-record snapshot, no leftover temporary
file, plus all prior stale/duplicate/rollback/locking/mode/kill-join checks.
Receipts are `classical-files-{host,arm}-live1.log`; daemon/ORT all-target
strict Clippy passes (`classical-files-clippy1.log`). This does not close the
original developer-only multi-tracker A/B modes.


The next automatic-owner source fixture captures 70 finite scenarios and 2,628
original process actions, including normal child replacement, TERM-resistant
kill/join, stable unexpected exits and Params failures with cleanup. The native
supervisor now matches all actions on HOST and actual ARM/QEMU against
independently executed source, including 204 floating wait values with zero bit
differences. Receipts are `auto-owner-host-source2/receipt.json` and
`auto-owner-actual-arm-source1/native-receipt.json`; observer all-target strict
Clippy passes (`auto-owner-clippy1.log`). The first comparison exposed the source
reading exit code twice before reporting a stable-child failure; native now
preserves that sequencing. Real process integration and main selection remain
pending. These test doubles grant no GPU ownership.

### Fresh raw Params acquisition audit (pending regression fix)

The source `read_mode` uses Cython `Params.get_int`, whose unchanged C++
`common/params.h::getInt` calls `std::stoi`. Two native readers instead accepted
only complete UTF-8 integer strings. A new original-binding fixture executes
1,009 valid-prefix raw values, including ASCII whitespace/signs, decimal tails,
embedded NUL and non-UTF-8 suffix bytes. The binding's numeric header and Cython
source hashes exactly equal final `e8e152db`; its recorded binary hash is checked.
Only Python logging is replaced to isolate these normal filesystem reads. The
reference is `mode-prefix-source3/{input,expected,receipt}.json`. Both actual
planner and observer regressions first failed against the old readers. Both now
reuse the existing verified Rust std::stoi-compatible integer helper. All 1,009
HOST observer results equal original Cython; native ARM matches that HOST
reference exactly (not an independently built ARM Cython binding). The actual
planner filesystem regression passes HOST and ARM/QEMU. Receipts are
`mode-prefix-observer-green1/receipt.json`, `mode-prefix-arm-receipt1.json` and
`mode-prefix-planner-{green1,arm-green1}.log`. Both crates pass all-target strict
Clippy (`mode-prefix-auto-clippy1.log`).

Malformed/out-of-range original Cython values can abort outside Python's
`try/except` (already covered by the retained beepd integer oracle). The Rust
fail-closed handling for those inputs is a retained native fix; valid-prefix
matching must not be described as original malformed-Cython equivalence.


### Real automatic child process boundary

`auto_process::ProcessOwner` now exercises the verified supervisor with actual
inert child processes on HOST and ARM/QEMU. Three scenarios cover replacement
through both backend labels, a TERM-resistant child requiring two-second
KILL/join escalation, a stable unexpected exit and Params failure cleanup. Every
old PID is reaped before return/replacement. The children never load a network
or open GPU hardware; a QCOM backend label here is only a transition input.
Receipts are `auto-real-{host,arm}1.log`. This proves the actual native process
boundary; the production adapter, main selection and installed QCOM provider
remain pending.

### Producer entry and QCOM work in progress

The production automatic adapter and typed entry are now registered and main
dispatch is wired. HOST and ARM daemon link, ARM `--help` executes, and the
observer passes all-target strict Clippy. The adapter uses the verified real process owner
and fresh Params boolean reads with source short-circuit ordering. Native Params
I/O errors remain errors that stop an old child, preserving the Rust error
contract instead of treating a failed DM read as permission to use the GPU.
The explicit CPU provider contract is `OPENPILOT_SIGNAL_ORT_ROOT` pointing to the
sealed ORT 1.23.2 extraction (`include/` and `lib/` checked against the pin), with
models under `/data/signal-color-shadow/neural/`. The internal child
protocol is `--backend-child tracking_cpu|qcom`; child returns bypass the manager
latch, which belongs to the parent.

Final tinygrad tree `2c7501785916e365318c31976d4aef5b6e44a332` differs from the
MDM runtime extraction (95 matching files, 109 different and 14 missing).
An exact final-tree reference is restored only in private planner scratch.
Skipping GPU submission while capturing source schedules would not establish
inference or authenticate precomputed weight transforms. No source signal QCOM
cache or genuine native signal QCOM graph is claimed; installed-cache conversion,
authentication and execution remain open. Missing-cache failures must not cause
a CPU neural fallback. Parent manager eligibility remains unselected.

Automatic adapter compilation and fresh read ordering now pass HOST and ARM.
The exact final `automatic_backend`/`read_mode` functions independently execute
480 scenarios per architecture, matching all 1,540 Params reads and their
short-circuit/error outcomes. Receipts are `auto-reads-host-source1/receipt.json`
and `auto-reads-actual-arm-source1/native-receipt.json`; observer all-target strict
Clippy passes (`auto-reads-clippy1.log`). This proves the production adapter's
read-policy component, not main selection or a hardware GPU handoff. A separate
39-case actual file-predicate reference now matches HOST and independently
executed ARM source for backend/flag whitespace, missing/directory paths and
UTF-8 decoding behavior. The initial native backend reader failed three control-
whitespace cases; source Python stripping is now preserved. Receipts are
`flag-files-host-green1/receipt.json` and
`flag-files-actual-arm-source1/native-receipt.json`.


Real typed-entry tests pass HOST and ARM/QEMU (`entry-real-{host,arm}1.log`):
Mode 2 forces classical even with an invalid neural backend file; explicit CPU
requires the authenticated provider; automatic mode starts a real classical
worker child despite `neural_enabled`; a normal comparison-change child return
is an unexpected-exit fault; missing QCOM caches cause the exact source cache
failure and child status 1. All child PIDs are reaped, and neither CPU neural
fallback nor GPU access occurs. Daemon ARM link/help and strict Clippy logs are
`entry-daemon-arm-build1.log` and `flag-entry-clippy1.log`. Installed QCOM-cache
execution is still explicitly unsupported and must not be hidden by this
missing-cache proof. Parent manager eligibility remains unselected.

A further final worker audit found that classical records and each transport
must recheck enable flags after processing; static `true` fields/unconditional
publication differ if mode is disabled during image computation. This race is
an open source-behavior gate; no preparation/departure threshold is relaxed.

The publication race reference now executes exact original record-flag and
publication AST statements: 32 cases, 128 production mode reads, disable between
record/start/prepare/display phases, and independently caught malformed payloads
(`classical-phases-source1`). The native path was extracted without changing its
old unconditional behavior so the next gate can first demonstrate RED. A real
VisionIPC regression also switches Mode 1 to Mode 2 inside Loaded: source freezes
`selected_mode` after Loaded/client construction, whereas the old native entry
froze it before initialization. Both repairs and their host/ARM reruns remain
pending. The old defensive neural-flag rejection inside the classical helper
must be removed now that typed entry owns selection; source tracking itself does
not watch neural flag changes.

Actual ARM provider build information narrows the outstanding template-score
gap: original wheel reports `4.13.0-1-gb4c5ec4042`, GCC 14.2.1; the retained native
provider reports GCC 13.3.0. Its `18d33121d-dirty` revision is the enclosing
openpilot-rust commit, not an OpenCV source revision. The restoration pins genuine
OpenCV 4.13 tag `fe38fc608f6acb8b68953438a62305d8318f4fcd`. The [official source
comparison](https://github.com/opencv/opencv/compare/fe38fc608f6acb8b68953438a62305d8318f4fcd...b4c5ec4042f097e2a5b386b9d413ec7333d0a184)
shows the wheel is one commit later, changing only two Python tests: core/imgproc
source is the same. Both report the same NEON dispatch and carotene/KleidiCV
0.7.0. Information was captured through actual ARM Python and an isolated native
C++ probe, without changing providers. Compiler/build configuration differs;
this is not yet a demonstrated cause or a repaired score comparison
(`opencv-{wheel,native}-arm-buildinfo.txt`, `opencv-source-diff.json`).


Classical publication/startup repairs now pass after captured RED regressions.
`classical-phases-host-red1.log` demonstrates unconditional publication was
incorrect. Production now independently rechecks record/start/prepare/display
flags in source order and preserves transport-specific error handling; disabled
outputs do not render malformed payloads. All 32 cases/128 mode reads and 136
floating fields match HOST and independently executed original ARM source with
zero bit differences (`classical-phases-host-green1/receipt.json`,
`classical-phases-actual-arm-source1/native-receipt.json`).

The real VisionIPC Loaded callback regression also first failed: native froze
selected mode before initialization. It now samples after Loaded, and the
unrelated neural-flag rejection inside classical tracking is removed because
entry owns backend selection. HOST and ARM real-worker lifecycle reruns pass
(`classical-startup-{host,arm}-green1.log`), retaining stale/duplicate/rollback,
exclusive owner, mode/comparison exit, atomic latest and kill/join checks.
Current real Entry children rerun HOST+ARM (`entry-after-phase-{host,arm}1.log`),
current ARM daemon link/help and strict all-target observer Clippy pass
(`classical-phase-daemon-arm-{build,help}1.log`, `classical-phase-clippy1.log`).
This closes classical publication/startup races on these corpora; additional
neural entry prerequisite/flag-error phases and fresh producer-to-planner IO
remain open. No installed QCOM execution, provider replacement or deployment
is implied.

The next dispatch prefix corpus is captured from unchanged final color/neural
`run` AST prefixes: 216 cases/440 reads, independently executed HOST and ARM
(`entry-phases-source2`, `entry-phases-actual-arm-source1`). It covers initial
Mode 2, both neural flag reads, requested short circuit, invalid backend and
UTF-8/decode failure ordering before model/tracker construction. A second corpus
captures neural record/start/display statements: 54 cases/126 reads with phase
disable, decode failure and malformed payload effects (`neural-phases-source2`,
`neural-phases-actual-arm-source1`). Native boundaries are extracted with their
old behavior retained for the next RED gate; these captures are not yet native
passing evidence. In particular installed QCOM remains unsupported.


Dispatch and neural publication phase repairs now pass after native RED captures
(`entry-phases-host-red1.log`, `neural-phases-host-red1.log`). Entry performs
requested/neural prerequisites before backend selection; its CPU owner consumes
that checked selection without repeating the prefix. The independent direct CPU
API retains its own prerequisite checks. Native flag readers now preserve
UTF-8 errors and Python whitespace rather than treating decode failure as false.
Record-flag decode failures end the worker; stopped-guard flag/render/write
failures are independently logged; display failures end the worker, as source.

All 216 dispatch cases/440 reads match HOST and independently executed original
ARM source. All 54 neural publication cases/126 reads, payloads, error effects
and 66 floating fields match with zero bit differences. Receipts:
`entry-phases-host-green1/receipt.json`,
`entry-phases-actual-arm-source1/native-receipt.json`,
`neural-phases-host-green1/receipt.json`,
`neural-phases-actual-arm-source1/native-receipt.json`.

The real CPU worker fixture now invokes typed Entry rather than bypassing it:
HOST actual model warm-up, active camera input, source payloads, duplicate/stale/
rollback, exclusive lock and disable cleanup pass (`neural-typed-host-live1.log`).
The independently executed ARM real typed worker also passes
(`neural-typed-arm-live1.log`), including exact source active tracks/payloads;
this is emulated lifecycle evidence, not target timing. HOST real automatic/classical children also rerun successfully
(`entry-after-neural-phases-host1.log`). Current daemon ARM link/help and strict
all-target observer Clippy pass (`entry-neural-phases-daemon-arm-{build,help}1.log`,
`entry-neural-phases-clippy1.log`). No installed QCOM path or fresh planner
acquisition proof is implied by these dispatch/publication gates.

The next fresh acquisition gate is staged, not verified: `SignalInputs` injects
owned Params, observation paths and an optional actual geometry subscriber;
production construction keeps its canonical defaults. The explicit
`signal-validation` feature records acquisition contexts, reader cache state and
actual receipt metadata only in the validation build. `signal_live_worker_trace`
and `signal_live_owner_trace` will connect a native Entry/classical producer,
real isolated VisionIPC, atomic endpoints, actual base/geometry message queues,
and `Planner::process`/wire publications. The frozen day-housing fixture has 24
NV12 frames (36,771,840 bytes) plus reference decoded RGB. Its first native run
must remain labeled acquisition evidence until original-source replay checks
all captured decisions/state/wire fields. No global endpoint writes are needed.


First connected acquisition execution now completes HOST and ARM/QEMU, all24
real VisionIPC frames through a native classical Entry child, atomic endpoints,
actual base/geometry queues, `Planner::process`, and actual wire publications
(`live-owner-{host,arm}-run1.log`). HOST has seven holds and ten green transitions;
preparation is inactive because these debug observations cross its original
freshness bound. That bound is unchanged. Native execution alone does not prove
source equivalence: the captured HOST producer tracker and both publishers now
match original source24frames/432floats bit-exact (`live-producer-host-source1/receipt.json`),
and independent ARM producer replay also passes432floats bit-exact
(`live-producer-actual-arm-source1/receipt.json`). Consumer acquisition, full owner
and wire replay plus active preparation and fresh-mode/fault cases remain pending.
Two-crate all-target strict Clippy passes with observer daemon and explicit
planner validation features (`live-owner-clippy3.log`). No bundle freeze occurred.

## Source-path completion ledger

Every row uses final commit `e8e152dbcfd9574a1791ed12026d1880e87689b3`.
A row describes verified functions/branches, not an automatic whole-file closure.
Temporary Python reference tools are still developer-tool conversion work.

| Exact source path | Native code / executable gates | Outstanding file scope |
| --- | --- | --- |
| `openpilot/selfdrive/carrot/signal_mode.py` | `plannerd/signal/transport.rs::{ModeCache,production_flag}`, observer `runtime::parameter_value`, planner `runtime/signals.rs::current_mode`; mode-constructor25/175reads, raw Params1,009 prefixes, fresh cache unit HOST+ARM | Audit independent full `flag_override` path-name corpus; original Cython malformed `stoi` abort is retained native fail-closed fix, explicitly not abort equivalence |
| `openpilot/selfdrive/carrot/signal_assist_runtime.py` | `plannerd/signal/transport.rs::Reader`, `runtime/signals.rs::Source`;520 transport/cache component cases and real connected acquisition | Retired analysis assist owner/record methods and actual captured consumer source replay; source membership does not cover retired helpers |
| `openpilot/selfdrive/carrot/signal_start_guard.py` | `signal/{start,color,transport}.rs`, observer `transport::start`, runtime `record_start`;2,880 full guard states,128 payload cases, real start endpoint, classical32/neural54 publication phases, full360 owner/wire source frames | Connected189-frame consumer and original record methods match HOST+actual ARM source; dedicated exceptional constructor/log-failure paths remain to audit before whole-file closure |
| `openpilot/selfdrive/carrot/signal_stop_prepare.py` | `signal/{prepare,geometry,priority,accel}.rs`;7,680 full decision states, geometry/corridor/night/alignment source corpus,360 actual final owner frames with98 actuating preparations | Final algorithm source file covered:7,680 full component decision states, strict576-case geometry and real189-frame active/fault owner paths exact HOST+actual ARM. Source pin e8e152db; vehicle acceptance separate |
| `openpilot/selfdrive/carrot/signal_stop_prepare_runtime.py` | `runtime/signals.rs::Source::prepare`, observer `transport::prepare`;576 current geometry-context cases, constructor25, classical32 publication phases, actual subscriptions | Connected189-frame active/moving-fault owner and original record methods pass HOST+actual ARM; exceptional lazy-subscription construction/logging paths remain for whole-file closure |
| `openpilot/selfdrive/carrot/signal_road_policy.py` | `signal/road_policy.rs`, runtime `suspended`; actual final owner has20 policy suspensions | Exhaustive standalone function path corpus and connected route/meta freshness cases before whole-file closure |
| `openpilot/selfdrive/carrot/signal_display.py` | Observer `transport::display`, source payload128 and neural display128/1,720outputs; root owns actual UI reader/continuity/projection | Coordinate root UI evidence for reader/continuity/project_box/record_draw; producer coverage cannot close reader half |
| `openpilot/selfdrive/carrot/carrot_functions.py` | Native `CarrotPlanner` + final longitudinal/lateral hooks;360 complete final owner states and every longitudinal wire field exact HOST/actual ARM | Fresh connected producer/consumer mode/geometry/record races; other non-signal Carrot helpers remain in planner ledger |
| `tools/signal_analysis/signal_tracker.py` | Observer `Process`, pixels/detector/evidence/tracker/causal-template;288 full private states/history hashes exact HOST/actual ARM; connected actual producer24-frame HOST source replay exact | Connected24-frame producer outputs match HOST+actual ARM including active-prepare housing scene; low-level OpenCV score discrepancy and developer tool conversion remain explicit |
| `tools/signal_analysis/neural_signal.py` | Observer neural CPU math/sequence/ORT;8calls113,612outputs,96 math cases,192 scripted +16 actual active causal states exact per architecture | Source QCOM session/cache/compiler path, authenticated A630 artifacts and native cache/export tooling remain unimplemented |
| `openpilot/selfdrive/modeld/signal_tracking_shadow.py` | `runtime.rs`, `worker.rs`, scheduling; real HOST+ARM VisionIPC stale/duplicate/rollback/lock/publication/stop lifecycle,32phase/128reads, Loaded mode race; new24-frame connected path | Full debug/analysis CLI branches and fresh connected owner/fault corpus; debug latency is not target performance |
| `openpilot/selfdrive/modeld/signal_color_shadow.py` | `entry.rs`, lifecycle;32 latch,216 prefix/440reads, actual classical/CPU child entry HOST+ARM | Legacy scan/artifact/analysis helpers plus complete CLI source closure; installed optional neural path remains separate |
| `openpilot/selfdrive/modeld/signal_neural_shadow.py` | `backend/auto*/entry/neural_runtime` and transport;70/2,628 supervision,480/1,540 fresh reads,39file predicates,216dispatch,54neural phases, real CPU and automatic children HOST+ARM | Installed QCOM caches/`run_selected(qcom)` actual model execution; no CPU fallback can close it |

Current whole signal subsystem status remains pending. This ledger intentionally
sends no blanket complete-file claim to the central inventory before the missing
branches and source gates above are resolved.


Original source consumer read/cache/current geometry context replay of the real
HOST capture now passes all24 frames and971 floating fields with zero bit
differences (`live-acquisition-host-source3/receipt.json`). It invokes unchanged
`SignalAssistRuntime.read` and `SignalPrepareRuntime.read`; captured clock and
receipt metadata drive the scoped queue-update doubles. Actual native transport
was already exercised in the connected run. Source constructors instantiate the
real signal classes; no geometry/context policy is replaced. Full owner/wire and
mode/fault expansion remain separate. Independent ARM consumer replay also passes24frames/971floats bit-exact
(`live-acquisition-actual-arm-source1/receipt.json`).


The connected HOST capture now also passes the original final full MPC/Carrot
owners: all24 owner snapshots, signal private state, parameter operations and
the entire actual longitudinal wire packet have zero mismatches
(`live-owner-host-source1/receipt.json`). The source recomputes the actual
captured read/cache/geometry context; owner constraint time and wall-clock calls
are explicit captured inputs. Actual native diagnostic durations are supplied
as publication metadata in the source writer, so this does not compare solver
or planner performance. The real initial payload read, subsequent cached JSON
reads, positive receipt ages, first-mode packet rejection, seven holds and green
transitions remain native execution effects rather than callback-only fixture
assertions. ARM capture2/independent full source replay, active preparation and
mode/fault expansion remain pending. This gate does not close installed QCOM.


Independent connected full owner replay now also passes actual ARM source:
24frames,24,336 floating fields, zero bit differences and maximum error0;
HOST has24,516 floating fields with the same zero differences
(`live-owner-actual-arm-source1/receipt.json`, `live-owner-host-source1/receipt.json`).
Counts differ because each architecture replays its own actual acquisition timing
and consequent state shape. This is same-architecture original source equivalence,
not a claim of identical cross-architecture timing. Both include original reader
recomputation, complete existing owner snapshots/parameter operations, all signal
private state and every longitudinal wire field. Only solver/planner/fast-radar
execution durations are declared captured diagnostic inputs; no speed claim.

The next expanded connected corpus is written but uncompiled:24 real producer
frames followed by38 explicit endpoint/vehicle/geometry/mode fault actions.
Fault rows are labeled separately from real camera frames; their exact endpoint
bytes and raw mode bytes are retained for original read-method replay. Duplicate,
reordered, stale and future green packets must not release the connected guard.
Normal production scheduling, freshness and eligibility thresholds are unchanged.
A worker-only opt-level3 validation profile is planned to exercise preparation
with current observations instead of interpreting debug-build expiry as active
behavior. It still requires same-architecture source comparisons and cannot be
reported as target performance or full subsystem completion.


Connected fault expansion now passes HOST original full owner replay:62 frames
(24 real VisionIPC producer frames plus38 explicit consumer fault actions),
63,240 floating fields and2,348 acquisition fields, zero bit differences.
Duplicate/reordered/stale/future green packets do not release the guard.
Worker-only optimization retained stale preparation; optimizing the native
OpenCV bridge package as well removes that expiry rejection. The repeated
HOST full owner replay remains exact (63,301 floating fields, zero bit
differences; live-owner-bridge-opt3-host-source1/receipt.json).
Active preparation is still unproved in this connected fixture: actual classical
tracks carry day_color_core evidence, whereas final prepare candidate selection
requires detected evidence. Source rejects these with no_red, matching native.
This is a corpus eligibility gap, not permission to broaden accepted evidence.
ARM connected fault execution/source replay remains pending. These package
optimization profiles are validation builds, not vehicle performance evidence.

Geometry primitive alignment has a separately retained numerical limit:
HOST signal-geometry-host3 and ARM signal-geometry-arm2 each contain1,552
floats with94 bit differences and maximum absolute error
3.552713678800501e-15 (alignment bound1e-12; range/night comparisons exact).
This is not bit equivalence; complete owner gates above remain independently
bit-exact on their respective source architecture and actual inputs.

The expanded62-frame fault capture also passes independently executed actual
ARM source:62,904 floating fields, zero bit differences, maximum error0
(live-owner-bridge-opt3-actual-arm-source1/receipt.json). Actual timing inputs
and state shapes remain architecture-specific.
A second real-pixel scene separates the housing from the source test's merged
background and increases lamp radius to4pixels. Original source and native
produce detected housing evidence and one prepare-confirming frame; subsequent
source cadence gaps use causal_template and are ineligible for preparation.
Full HOST62-frame owner/wire replay remains exact:63,305 floating fields
(live-owner-housing-host-source3/receipt.json). The first source reference run
exposed a missing itertools.combinations import in the scoped AST loader, fixed
without changing the original class or native runtime. Active connected prepare
remains pending; no acceptance criteria were relaxed.

Geometry alignment rounding diagnosis: independently executed HOST and ARM
source3x2 matrix products each compare3,456 ray components. Existing scalar
accumulation differs in528 components; sequential nested fused multiply-add
matches every component exactly. Native alignment now uses this accumulation
only for the matrix path, retaining existing scalar range/night projections.
The geometry oracle now requires zero tolerance for all three outputs. Native
HOST/ARM rebuild and exact full geometry regression are pending the next slot;
previous94-difference receipts remain historical RED evidence, not repaired proof.

Native OpenCV ownership adapter optimization is written pending compilation:
byte images, i32 labels and Float32 score planes use one checked contiguous
slice copy into Rust-owned vectors, replacing per-element CXX Vec push calls.
OpenCV operations and algorithms are unchanged. Noncontiguous outputs still
clone before copying; label/score element types are checked. Existing native
primitive output is captured independently on HOST and ARM before rebuilding
(opencv-bulk-before-{host,arm}1.json) for exact before/after comparisons, followed
by source pixel/tracker and real connected owner gates. No speed claim or
selection change is established by this uncompiled edit.

Actual frozen ONNX graph inventory for the pending native QCOM scheduler:
detector opset11 has277nodes/174initializers:Conv83,Sigmoid80,Mul74,
Concat18,Add7,Slice6,MaxPool3,Reshape3,Resize2,Transpose1. Classifier
opset11 has171nodes/106initializers:Constant70,Conv52,Clip35,Add10,
GlobalAveragePool1,Flatten1,Gemm1,Softmax1. Inventory was read from the
verified original model bytes with external ONNX1.20.1 (temporary reference
tool only), saved in signal-qcom-onnx-operators.json. Existing native QCOM
program/kernel compiler accepts pre-scheduled graphs; metadata parsing or
compiling individual kernels does not yet implement this graph schedule,
source JIT cache import or authenticated initialization/inference.

Latest repair gates now pass: geometry576cases on HOST and independently
executed actual ARM source, all1,552 floats bit-exact (range392/night584/
alignment576), zero tolerance and zero maximum error. Receipts:
signal-geometry-strict-{host,actual-arm}-green1/receipt.json. Prior scalar
94-difference alignment result is superseded for these same cases.
OpenCV bulk-copy25-case primitive serialized outputs are byte-identical before
and after on each architecture. HOST contracts3+native3 pass; actual ARM
native3 pass, including input-drop/tensor ownership with verified final lane
model SHA256d3761c185daf33897ff3ff5edf28115c5dc829c06c538033accb616d6c67528c
(raw Git blob09192b6f5fb62031b4d1f563e22c9219415bced1; git hash-object must
use --no-filters because checkout LFS clean filters return a pointer hash).
Strict all-target Clippy passes opencv-runtime/plannerd/signal-observer. Initial
ARM example linking lacked libz.so.1 search; scoped rpath-link fixed linking
without changing sealed providers. Initial HOST native test missed its required
model environment; rerun with the verified actual model passes all tests.

Connected active preparation is now reached by extending isolated-housing red
dwell to18 real frames (24total plus38consumer faults). Native HOST has3
active red_prepare frames; actual final-source tracker/publishers match24frames/
384floating fields, and full owner/wire replay matches62frames/63,405floating
fields with zero bit differences/maxerror0. Receipts live-producer-bulk-longred-
host-source1 and live-owner-bulk-longred-host-source1. All observation
freshness, source evidence restrictions, confirmation dwell and scheduling
policies remain unchanged. Actual ARM active-scene execution/source replay is
still pending. No debug timing is presented as vehicle performance.

Active connected ARM scene now also passes independently executed actual final
source:13 active/control frames,7 capped frames,62 complete owner/wire frames
with63,423 floating fields, zero bit differences/maxerror0. Its original
tracker/publishers24frames/384fields also match exactly. HOST has3 active/
control/capped frames and63,405 fields due its own measured cadence; these
are separate architecture-local source comparisons, not cross-architecture
time equivalence. Receipts live-owner-bulk-longred-actual-arm-source1 and
live-producer-bulk-longred-actual-arm-source1.

Remaining source-file completion detail: complete owner snapshots currently
exclude runtime decision-log suppression state. A validation-only start-record
clock input event and optional --record-events source replay are now written
to exercise original SignalStartRuntime.record and
SignalPrepareRuntime.record_prepare and compare every emitted field/order.
These changes await a fresh connected capture/rebuild; no full source-file
completion is asserted before that gate. Moving active preparation invalidity/
reacquisition cases also remain separate from the stopped guard fault corpus.

Moving active-fault corpus now passes HOST189 complete owner/wire frames:
192,805 floating fields, zero bit differences/maxerror0; acquisition6,900
fields exact. There are24 real producer frames plus165 explicit moving
consumer actions (ten fresh-red reacquisition observations before each of15
invalidity/transport/mode actions),60 active decisions and56 actuating frames.
Invalid camera/calibration/pose/vehicle, stale pose/camera/vehicle and turning
all deactivate preparation; malformed/missing/version-fault endpoints deactivate
it. Source original record methods also execute with captured exact record
clock inputs and independently recomputed source speed/stop distance:
23 signalStartGuard and50 signalStopPrepare events match every field/order,
including suppression and observed-mode control_active handling. The source
reference uses --record-events; source classes are unchanged. Receipt:
live-owner-preparefault-host-source1/receipt.json. ARM189-frame capture/source
comparison remains pending. Native HOST+ARM examples and strict all-target
planner Clippy pass after the validation trace extension.

Moving active-fault189-frame actual ARM source comparison is now GREEN:
192,850 floating fields, zero bit differences/maxerror0,83 active decisions/
78 actuating frames; original runtime record methods emit32 start and51
prepare events with exact field/order/suppression behavior. HOST counterpart
is192,805 fields/60active/56actuating and23start/50prepare events. Both
architecture-local inputs and complete original owner/record paths are retained;
no cross-architecture timing identity is asserted.

Narrow source-file algorithm closure at e8e152dbcfd9574a1791ed12026d1880e87689b3:
openpilot/selfdrive/carrot/signal_stop_prepare.py maps PrepareDecision,
SignalStopPrepare,PrepareAccel to plannerd/signal/{prepare,accel}.rs;
road_corridor,housing_range,night_lamp_support,housing_alignment map to
signal/geometry.rs; central_red_priority maps to signal/priority.rs. Evidence:
7,680 component full decision states per architecture;576 strict-zero geometry
cases/1,552floats per architecture;360 full owners with98 actuating preparations;
connected189 real-IO/fault owners with actual ARM original-source/record replay
as above. This closes this algorithm file, not colocated model/runtime/export
files, unverified exceptional lazy-subscription/logging construction, installed
QCOM, final package freeze or vehicle acceptance.

Native QCOM tooling primitive stage: std-only standalone regression first
failed against the old reader's missing plain-entry/Float APIs
(qcom-pickle-red1.log). Shared hcq_import/pickle.rs now has a separate
bounded protocol4/5 plain entry and caller-owned global whitelist. Original
AMD parse/metadata whitelists/opcodes remain unchanged. Extended BINFLOAT,
BYTEARRAY8 and one-element APPEND/SETITEM are enabled only for plain input;
plain persistent buffer IDs are rejected. Signed zero/NaN payload bits, raw
bytearray extents and memo identity are preserved, floats never coerce to ints.

New dev-tools signal_qcom_cache::inspect validates the outer
(input_names,TinyJit(None,CapturedJit(...))) shape and reports inert constructor
names/float bits/byte extents. Its separate final-source whitelist does not
execute constructors, authenticate provenance, lower/initialize kernels or
provide inference. Native primitive3 tests, two protocol4/5 structural envelope
fixtures and strict all-target dev-tools Clippy pass HOST. Envelope fixtures use
actual final TinyJit/CapturedJit/UOp classes with an empty SINK graph and no
model/GPU: final jit.py Git blob885cc97e01cca9363250a4b7386b9771594fc83d
was checked byte-exact against e8e152db; tinygrad tree2c7501785916e365318c31976d4aef5b6e44a332.
Actual799,942,179-byte AMD MDM artifact SHA256
256f2178366479a211933701c03518621ed3970289e6a8a43e5a2bcb15e55b5f
still produces exactly the reference descriptor SHA5f6665848de4e353e0ff0c6f7a68880a195a9e217b33317fd9ef712ee30e2133
and metadata SHA2199a21fdc9cc53eca0606c85c2ea8b2cf051091e73fa81edfd5b3259b6ca822
with all required fixture environment supplied: HOST4 tests GREEN50.79s.
Fresh ARM lib harness is linked; actual MDM import regression executes
independently under QEMU. This is an AMD compatibility regression, not signal
QCOM model execution. Metadata identity validation, full captured graph
constructor/schema lowering, authenticated A630 artifacts and inference remain
open and producer selection stays gated.

The restricted QCOM-envelope reader's actual CLI now passes on host and
ARM/QEMU, including protocol-4/5 envelopes, truncation and rejected foreign
constructors without a Python interpreter on PATH (`qcom-cache-cli-host1.log`,
`qcom-cache-cli-arm1.log`). Shared-parser preservation was additionally checked
against the full existing 799,942,179-byte authenticated AMD MDM artifact:
all four import tests pass on host (50.79 s) and ARM/QEMU (733.60 s), comparing
both descriptor and worker metadata references with all required fixture
environment variables set (`qcom-pickle-mdm-host1.log`,
`qcom-pickle-mdm-arm1.log`). This closes neither QCOM graph lowering nor
A630 kernel authentication, scheduling, initialization or inference.

## QCOM artifact acquisition and admission boundary (2026-10-16)

The final pinned `tools/signal_analysis/neural_signal.py` constructor checks
adjacent `*.qcom.pkl` and `*.qcom.json` against model SHA, compiled SHA and
`qcom_runtime_identity()` (tinygrad tree, Python ABI, machine, GPU and six QCOM
flags). Its line62 explicitly requires locally compiled artifacts and forbids
pickle downloads. The pinned tree contains no signal PKL/ONNX or compiled cache
manifest; the available runtime-provider/cache assets contain neither required
signal PKL. The two original pinned ONNX inputs are already downloaded and match
`rust/upstream/signal-runtime.json` (detector35,794,092B;
classifier8,926,508B), so repeating that acquisition does not produce QCOM caches.

The absent production inputs are the two locally compiled final-tree A630 caches
and their model/runtime/GPU identities, or equivalent graphs from a complete
native ONNX-to-QCOM scheduler. Native cache decoding/lowering and session wiring
remain engineering work here. Production admission must bind converted graph,
weights and kernels to the authenticated runtime package manifest; a JSON file
containing its own hashes is not kernel authentication. Physical GPU execution
is user-owned, while host/ARM parser, lowering, admission, lifecycle and packaging
checks remain required engineering gates. The new final-source structural graph
fixture uses explicitly supplied inert marker bytes and a genuine existing
compiler binary: it performs no GPU work and supplies no inference evidence.

The required-versus-optional distinction follows the actual final source:
`signal_neural_shadow.neural_requested()` is false when its enable file is
missing/unreadable, and `backend_requested()` defaults to explicit CPU when its
backend file is absent. Ordinary mode/classical operation therefore does not
require QCOM caches. For opt-in auto/QCOM, `run_selected()` requires all four
local cache/metadata files and raises on absence. It does not use cache failure
as a reason to select tracking CPU; `automatic_backend()` selects tracking CPU
for preparation, driving-GPU use or required DM, and QCOM only when external GPU
is active and DM does not need the internal GPU. Native dispatch preserves this
policy. Missing authentic QCOM inputs block that enabled optional feature,
not the ordinary classical producer; they remain explicit pending assets.

## Native QCOM lowering, package admission and session gate (2026-10-16)

The separate data-only QCOM decoder now lowers final TinyJit constructor data,
PARAM bindings, backing buffers/views, COPY and PROGRAM calls into the existing
native `QcomGraph` format. It uses `model-runtime::export::BufferTable`, requires
real initialized bytes for reads before writes, checks PARAM byte contracts,
preserves kernel bytes/argument/image/launch metadata, and rejects unsupported
symbolic/device/opaque storage contracts. Protocol4 bytearray constructors are
inert names handled only by the QCOM whitelist/decoder; AMD pickle policy and
reader behavior are unchanged. The original-final-source structural fixture
contains explicitly supplied marker storage and the existing genuine compiler
binary; these tests perform no model computation.

`convert-signal-qcom-cache` validates local cache/model/runtime metadata, writes
a new private native bundle, validates genuine descriptors/arguments/dispatch,
records file identities and publishes without replacing an existing tree. The
disk reserve is checked before output growth. Corrupt output is rejected and the
private temporary tree is removed. Its receipt explicitly requires an enclosing
verified runtime package; it does not authenticate its own model semantics.
The actual host/ARM CLI help and identity-rejection checks work with no interpreter
on PATH (`qcom-convert-cli-smoke1/receipt.json`). A complete real-model positive
cache conversion remains pending authentic cache data; structural bundle writing
is separately tested with the genuine small kernel.

The launcher's existing manifest policy is now one shared implementation in
`runtime-package/src/manifest.rs`, included by launcher `package.rs` using the
original launcher Error type so its public API/diagnostics remain unchanged.
`Manifest::read_with_digest` binds one bounded snapshot; bootstrap passes its
already verified digest and root only into the manager command. The shared
`VerifiedManifest` repeats the same identity/file policy, rejects stale/changed
manifest/files and admits bounded immutable artifact snapshots. It does not
claim an arbitrary caller-provided digest is an authentication authority.
QCOM then loads the package-bound graph via `QcomBundle::load_verified`; the
unsafe `QcomModel::from_bundle` consumes that same loaded snapshot before opening
the device, avoiding a graph re-read across the admission boundary.

The optional provider admits both detector and classifier before opening the
first driver. Fixed shape/name/byte contracts, synchronous write/run/read order,
Float32 NaN/signed-zero preservation, session poisoning and RAII cleanup are
covered with an explicitly inert factory. Six warmups run through both adapters;
a second-driver failure releases the first, and a changed second graph rejects
before any factory opens. The fixture's buffer-add kernel is explicitly neither
real signal model and supplies no inference evidence. Selected-worker/daemon
integration and authentic optional model-cache assets remain pending.

Evidence in the planner scratch directory: shared token3 tests
`qcom-package-token-{host,arm}2.log`; unchanged launcher6 tests
`qcom-launcher-package-{host,arm}1.log`; lowerer/packager5 tests
`qcom-packager-{host,arm}2.log`; provider/session2 tests
`qcom-provider-host2.log` and `qcom-provider-arm2.log`; strict five-library lint
`qcom-package-provider-clippy3.log` and converter lint
`qcom-convert-cli-clippy1.log`. ARM runs use actual linked binaries under QEMU.
No KGSL device or physical vehicle is opened by these fixture gates.

## Corrected #338 runtime-only QCOM boundary (2026-10-16)

The user separated development/compiler/tool conversion into another session.
No further QCOM compiler, pickle-conversion, artifact acquisition or broad
validation work is part of this track. Previously written files are preserved.

The optional QCOM runtime introduced in the original changed range is now wired
through `signal-observer::entry` and the existing neural frame owner. The owner
is generic over its already existing Network interface; explicit CPU selection
still uses ORT, and automatic/QCOM selection uses the existing native QCOM
provider without a CPU fallback. Production first requires both native cache
pairs, then the launcher's expected runtime-manifest SHA/source/file checks,
model/binding contracts and immutable QcomBundle snapshots. Both model bundles
admit before the first model opens; existing session ownership handles cleanup.
The selected loop performs the same warmups, camera capture/tracker/publication
and mode/backend lifetime checks as the previous CPU owner.

Required native cache location is
`rust/assets/signal-qcom/{detector,classifier}/`, with `graph.json`,
`signal-qcom.json`, weights and authenticated kernel files covered by the verified
runtime package. An original Python PKL/JSON cache by itself is not a supported
native runtime artifact; its conversion belongs to the separate tooling session.
No authentic converted signal cache is available here. This limits the optional
QCOM feature's positive inference evidence; it does not prevent default classical
operation. No inference equivalence or physical GPU result is claimed.

Focused evidence: daemon with both `daemon,neural-qcom,neural-ort` features compiles
on host; four cache-presence/session/admission tests pass with an explicitly inert
factory (including six warmups, second-model failure cleanup and package tamper
rejection before opening). The selected ARM signalcolord binary links with the
existing AGNOS provider's transitive libz search path; the ordinary SDK-only link
first failed because it lacked that native provider dependency. Existing actual
entry example was linked with the same features and run under QEMU against an
isolated empty cache: exit1 and exact source missing-cache rejection, with no GPU
open or CPU fallback. Receipt is
`.analysis/scratch/2026-10-15-dev-md-planner/qcom-selected-entry-final/receipt.json`.
No broad regressions or new tool/converter build was run in this corrected gate.
