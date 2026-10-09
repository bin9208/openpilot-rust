# USB/AMD native model provisioning checkpoint

Issue: [#154](https://github.com/bin9208/openpilot-rust/issues/154).
Source: `openpilot/selfdrive/modeld/{big_model,precompiled_model,big_model_status}.py`.
This is a host provisioning/readiness checkpoint. Normal-startup orchestration,
remaining runtime formats, target package acceptance and device acceptance remain open.

## Artifact and runtime boundaries

The native downloader preserves the existing model manifest, state/cache names,
catalog URLs, size/hash verification, resume policies, old-active retention and
internal-model fallback. The verified `runtime.tar.gz` is retained as provenance
data; its Python files are never imported or executed by native model delivery.
Archive extraction uses the original data-filter destination and permission rules.

The execution provider is the existing `usbgpu-assets/manifest.json` package keyed
by the selected pickle SHA. It must contain the HCQ descriptor and worker metadata,
both AMD warps, both QCOM graph/kernel/weights/provenance sets, the gfx1200 probe,
the pinned firmware and notices. Required consumer metadata is validated even if
a malformed manifest omits entries. This checks trusted-package integrity, not
signature authenticity or physical GPU operation.

`model::Paths` now carries the asset directory explicitly. Drivingd and the UI
use the runtime executable's `usbgpu-assets` sibling. Native status uses catalog,
model-size and companion validation. Full pickle/archive hashing remains in
boot/ensure, matching the source's separation from ordinary status reads. Companion
validation cost has not been measured as a vehicle performance result.

Python import files and local SCons `.chunkmanifest` files alone cannot mark a
native model ready. The retained filesystem matrix still records every original
observation: 192 scenarios, 158 unchanged comparisons and 34 explicit provider
migration differences. Those differences are not source parity. Real native
ready/missing/corrupt package controls are separate local-only acceptance evidence;
the generic workspace job does not construct the genuine positive provider yet.

## Actual host checks

Receipts are under the owned worktree's ignored
`.omo/evidence/154-runtime-resume/` directory. Each invocation contains command,
exit status and raw stdout/stderr; binaries are identified by SHA in the results.

| Boundary | Scenario and binary observable | Captured evidence |
| --- | --- | --- |
| Delivery | 18 owned HTTPS download/resume/error/partial cases; request headers, progress and files match | `model-delivery-v2/result.json` |
| Metadata | 31 manifest/state/dotfile/catalog cases; original parsing/selection matches | `model-metadata-catalog-green/result.json` |
| URL composition | Uppercase hostname and explicit `:443` survive manifest resolution through catalog validation | `provision-authority-green/result.json` |
| Native package | Missing QCOM manifest closure and valid-hash/wrong probe architecture reject; restored package succeeds | `provision-assets-green/result.json` |
| Archive | Nine actual source extraction comparisons including safe absolute/dot-dot, mode handling, escape/link/special rejection and expanded-size limit | `archive-data-filter-green/result.json` |
| Request/phase composition | Three original verification-callback sequences plus rejected-marker catalog request with OS trust and identity encoding match | `provision-delivery-green/result.json` |
| Installed marker | Actual original ensure/native install agree on marker and progress with the real pinned PKL hard-linked; old header difference is superseded by the affected request control | `precompiled-install-v1/result.json` |
| Runtime readiness | Public `model::status`/`active_compiled_path` report ready for the complete package and unavailable for missing/corrupt companions | `provision-readiness-green/result.json` |
| Status/boot hash | Ten persisted phase/throttle events match; same-size sparse corrupt model is rehashed before the owned failed retry request | `provision-phases-v3/result.json` |
| Failure receipts | Fourteen original/native cases match rejection, receipt removal/preservation and 16,384-code-point error truncation | `provision-failure-final/result.json` |
| Required checks | Selected builds, all-target strict Clippy, two model tests, caller-library check, fmt and Python gates | `provision-checkpoint/`, `readiness-build/` |

Earlier malformed-CA, dotfile, array-shape, catalog normalization, archive,
missing-QCOM and fixture setup failures remain recorded. Numerical tolerances are
unchanged. The 34 provider-migration outcomes use explicit native expectations
while retaining the original results. The current full-model numerical proof
is documented separately in `rust-usbgpu-emulator-alignment-260.md`; it was not
repeated for this safe provisioning boundary.

## Build preparation and remaining execution formats

The approved port design permits explicit Python **build-time** model conversion
and test oracles. The original ONNX compilation is called by
`system/manager/build.py::build_usbgpu_model` during launch build preparation,
before manager startup; the pinned precompiled PKL bypasses it. This does not
authorize Python calls from the installed native inference worker.

The generic PKL's fused HCQ graph and packaged AMD/QCOM warps have native consumers.
The `comma-run-model` per-camera `run_model`/input-queue contract and the local
`run_policy` plus per-camera TinyJit warp contract still need native companion
exporters, queue adapters and verified build/package orchestration. A general
ONNX/Tinygrad graph compiler rewrite is not required by the approved design.
Downloaded `make_input_queues`/`MODELD_INPUTS` Python code and runtime warp
compilation must not become hidden native-runtime dependencies.

The new status and failure receipt modules are verified boundaries; full native
boot smoke/validation-cache orchestration and normal-startup wiring are the next
implementation work. This checkpoint does not claim those integrations completed.

The historical `drivingd/examples/usbgpu_selection_trace.rs` fixture creates a
synthetic local ONNX and `.chunkmanifest`, without native companions. Its unchanged
positive normal/grace/retry/disconnect assertions cannot pass under native
readiness. The constructor/library compilation check does not validate those
branches. Their replacement remains tracked in #154: use a genuine converted
local `run_policy` package and native queue adapter when that path is implemented,
then rerun the affected original/native selection and warm-fallback controls.
Keep the positive assertions; do not introduce a fixture readiness override or
count the synthetic Python build marker as a native provider.
