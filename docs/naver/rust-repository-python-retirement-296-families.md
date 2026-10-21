# Remaining functional closures for #296

The user's 2026-10-22 direction keeps strict vehicle/control/safety checks and
batches general development tools by functionality. Reuse successful evidence
unless relevant source, dependency, feature union, provider, environment or
target changes, or a new failure gives a reason to rerun it. Retain final
required integration checks. Do not run a heavyweight suite per removed file.

This ledger tracks functional closures, active dependencies and blockers.
The [routing map](rust-repository-python-retirement-296-families.tsv) covers
every remaining pending/partial baseline entry exactly once. Its counts classify scope;
they are not completed functionality. All families below still contain work.

| Family | Remaining dependencies | Completion conditions |
|---|---|---|
| Vehicle/control/safety | opendbc platforms and safety; Panda codecs/transports; Card; planner/MPC; radar; pose; control/monitoring original oracles | Preserve platform/case coverage, negative cases, tolerances, cadence and validity. Native implementation and actual callers, independently sourced comparisons, original-source absence, affected target checks. User vehicle acceptance stays separate. |
| Tensor/compiler/model export | tinygrad graph/schedule/lowering/backend APIs, autograd/training, optimizers, data/examples/tests; model export and offline compiler chain | Source-to-model compilation, kernel/provider provenance, metadata, numerics and failures for each supported backend/graph family. Frozen compiled-model consumption alone does not retire a compiler. |
| Runtime/model/media comparators | UI/cluster, camera/webcam, Xiaoge/OpenCV, model inputs, navigation, logging/telemetry, startup/services, sensor/GNSS/platform | Use existing job boundaries. Convert controller plus shared fixture/source adapters together; migrate all consumers and preserve success/failure/cleanup observations. Hardware/model validity retains strict affected checks. |
| Web/recovery/upload | HTTP/WebSocket/PTY/process peers; recovery controllers; shared upload/save/media/network and Params helpers; web terminal/configuration/profile/Bluetooth routes | Native wire/body/frame/error behavior, disconnect/retry/timeout and child ownership, followed by actual caller and shared-helper removal. One bounded gate per compatible route/controller group. |
| Replay/developer/docs | route/log reader/schema/cache; car-porting and notebooks; map tools; Android/Jetlink conversion; docs/wiki/renderers | Equivalent runnable workflows and failure effects. Full schema/service coverage and replay data semantics remain; do not activate inherited publication/deployment workflows to validate a port. |
| Build/generation/staging | Params/MsgQ/VisionIPC binding builders and consumers; generated vehicle/schema/pose tables | Native build/schema/dependency graph, exact relevant commands/manifests and early-failure cleanup. Reuse accepted JSON11/media/provider stages. Params still invokes Cython; its native JSON11 dependency alone does not close that builder. |
| CI/release/launch | remaining Python tests/controllers; AST syntax admission; release/sealing/source packaging; watchdog/bootstrap and embedded scripts | Preserve assertions, exact artifact/source binding and fail-closed release behavior. Native AST-equivalent admission or final Python removal; actual caller transition and required PR/postmerge gates. No publication is authorized merely by this ledger. |
| Other vendor infrastructure | MsgQ/VisionIPC APIs/tests, symbolic filter generation, teleoprtc and Jetlink transport | Preserve licenses and API behavior, migrate consumers and tool functionality. A Rust launcher around Python does not close the dependency. |
| Cython/SCons | root/module build graphs, extension declarations and consumers | Native build discovery, clean/incremental/error behavior and replacement APIs for every consumer before removing original bindings/build sources. |

Additional scope outside the baseline source inventory remains explicit:
`selfdrived_helpers_source.py`, `tests/test_selfdrived_source.py`,
`tests/test_params_native_stager.py` under `rust/tools`, six car-porting notebooks,
and the 53 currently recorded pending entries in the [execution-edge inventory](rust-repository-python-retirement-296-execution-edges.tsv).
Those include generated extensionless helpers, dynamic launch, embedded Python
and vendor release/build paths. The22October follow-up adds34 explicit sites
(30 missed paths, one generated-exporter subedge and three bootstrap retry sites).
This remains an observed inventory, not an exhaustive interpreter-free proof. Relocation and missing files do not close the
three partial baseline entries.

## Current batches and blockers

1. Panda device/protocol/CAN-I/O source deletion and maintained fixtures are
   accepted in39640526320 and39647358174. The shared corpus is removed with both
   native callers. Complete independent observations and parser/failure effects
   are preserved. Final actual outer38-phase/two-root integration must cover
   the163-source/34-executable current composition; its14 mock controls alone
   do not replace that gate.
2. Driving/driver/logging faults: complete fixture/source-absence/finalization
   and optional-collector candidates are committed. Driving native collector
   and both private-root functional lanes passed. The cross-root metadata failure
   was isolated to one exact DEBUG camera-wait message; only that already-accounted
   diagnostic count is normalized. Four original controllers are now deleted and their maintained callers migrated;
   the final complete comparison and eight reappearance controls pass39663027128. The compact independent
   fixture preserves1649 original file pins in59,663,776 XZ bytes; native full
   regeneration/export passed39654500423. Its private fault-builder failure #473
   is now diagnosed and fixed:39661522311 passes8 units, strictClippy and actual
   byte-identical restoration with all locks/source seals unchanged. The final
   retained replay and full two-root traffic also pass39663027128. Driver's actual collector CLI data is
   independently auditable despite a wrapper row-count failure; do not repeat
   unchanged traffic solely to fix that assertion. The reused driver catalog failed warmup with confirmed SIGILL: AVX-512 objects
   were run on an AMD EPYC host without that ISA. CPU target compatibility is a
   real reuse boundary. Use current-host exports, preserve historical objects
   and source seals. The first fresh-target attempt39651208682 stopped before
   model traffic on a new shared stager Clippy lint;481ae6fe fixes it without
   changing runtime policy. Current-host gate39652828256 now passes
   no-collector/CLI/raw-alias comparisons; parent and reviewer audit all535 ZIP
   CRCs and both43-file original captures. Shared driver source/caller retirement
   remains pending.
3. Full f5c8 CI `39617311670` passed all runtime/ARM jobs except workspace:
   `desire_reference_cli` failed a source-output hash assertion. The serde feature
   ordering defect #463 is fixed and affected gate39639003843 is accepted. Preserve
   successful checks and retain required final integration.
   The aggregate remains failed. Navigation #457 also remains open because its
   earlier startup stall has not been explained by successful diagnostics.
4. Recovery's eight configured controllers share HTTP/WebSocket/PTY/process
   helpers. The independent fixture now contains213 records (207 original
   observations, one controller receipt and five explicitly native-only shutdown
   contracts); data completeness does not mean native acceptance. The13 Python
   controllers/helpers remain until all eight native paths and both roots pass.
   Frozen gate39663706166 passed six controllers in both roots and identified
   actions directory ownership and upload fixture setup defects. The repair
   preserved source semantics. Traced39667756626 then stopped in strace's
   PTRACE_LISTEN/EIO path (#474); untraced39669218530 separately exposed the
   action executable/sibling layout and one owned missing-file error path.
   Those failures are distinct. Commit047b1ea40 binds the action executable and
   process-child as held same-inode aliases and projects only the independently
   recorded missing-file absolute path. Full untraced and traced eight-controller
   rerun39672931897 is pending; no Python deletion is accepted yet. Complete
   execution classification, old bootstrap API cleanup and actual caller migration
   remain gates after functional success. Literal WebSocket mask/boundary tests
   and frozen webhook bytes are already native and must remain intact.
5. Startup/USB daemon comparator ports are registered in4a3cad747. They reuse
   authenticated current-host catalog admission, reap descendants after leader
   exit and reject replaced/modified success reports. Hosted compile/regression/
   strict-Clippy preflight39673003192 is pending. Independent original capture,
   complete two-root functional comparison and source absence remain required;
   driver/shared logging sources stay until these consumers are retired.
6. Teleop's two embedded release computations are accepted in39670630661:
   independent34 computations and10 disposable Git scenarios match. The native
   CLI and maintained script caller are present; main CI policy integration
   remains pending. This closes one execution edge without changing the baseline
   source count.

## Evidence reuse and delivery

Key reuse to relevant source/fixture/ABI hashes, compiler/provider/target and
features, original ZIP/manifest/member seals, normalization/error contract, and
actual caller/environment. Keep historical capture identities; do not relabel
old data as freshly captured or use native output as its own expectation.
Preserve exact required safety and final integration checks while grouping
ordinary tools. Earlier accepted updater, Panda outer, encoder, JSON11 and
other closures remain in the [evidence ledger](rust-repository-python-retirement-296-evidence.md).

Local work stays at syntax/format/diff and stored evidence inspection; execution
uses GitHub-hosted Actions. #296 stays isolated from dev-md/#338. The user will
pull and personally check normal boot, recording, upload and driving; no such
device acceptance is claimed here. Whole-repository conversion remains open.
