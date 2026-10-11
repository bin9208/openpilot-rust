# Native staged AMD local-graph checkpoint (#154)

The native AMD graph runner now consumes ELF kernels, verified initial data,
aliased views, copies and ordered entrypoints from an explicit build-time export.
`Bundle::open` checks descriptor limits, weight/kernel identities and initial
weight ranges before GPU allocation. `Graph::from_bundle` then loads that verified
contract into the existing native AMD GPU runtime. Invalid execution or binding
operations stop the graph instead of continuing with partially updated state.
The worker and model-selection paths are not changed by this checkpoint.

The exporter resolves the original captured call and buffer relationships; it
does not reconstruct trained network math. The owned fixture serializes captured
`make_warp` and `make_run_policy` TinyJits using the original OOB format, reloads
them through `model_export.original.driving`, and exports their separate prepare
and policy stages. Its arithmetic model runner consumes both camera images and
feature/desire/control history. It is a small actual source graph, not a trained
model or a trained-model numerical acceptance result.

## Observed current controls

Private receipts are relative to this worktree's
`.omo/evidence/154-runtime-resume/local-graph/` directory. Commands, environments,
exit codes, outputs and exact source/binary hashes are retained in those receipts.

| Scenario and invocation | Binary observable | Artifact |
| --- | --- | --- |
| Capture and source: `check_usbgpu_local_graph.py --kind capture/source --evidence .../resume-v4` | Real serialized artifact reload/export succeeds; CPU source emits changing main/wide NV12 inputs, transforms, queue snapshots and recurrent outputs | `resume-v4/{capture,source}-invocation.json` |
| Owned GPU: same command with `--kind native --binary .../gpu_graph_rpc --firmware .../firmware` | 27 stage calls (15 prepare, 12 policy), 261 actual owned gfx1200 kernel dispatches; all 161 source/native byte comparisons equal and all float outputs/history finite | `resume-v4/comparison.json`, `native-final-invocation.json`, `trace.json` |
| Native admission: `cargo test -p openpilot-usbgpu --lib model_graph::tests --locked --offline` | Eight cases pass: stage partition; malformed identity; overlapping stages; out-of-range views; copy-length mismatch; corrupt kernel; missing initial weights; validated binding sizes | `resume-v3/tests.json` |
| Native build: selected example build and package all-target strict Clippy | Both exit zero; current executable SHA is `d69e88dd78c2d29fc29f46da9df047750942712c31f352fa629cafeae278cfbf` | `resume-v3/{build,clippy}.json`, `resume-v4/source-hashes.json` |
| Export boundaries: scoped `pytest rust/tools/tests/test_amd_export.py` | Both AMD alias/queue and host-graph-rejection cases pass | `resume-v4/python-tests-scoped.json` |
| Helper checks: Ruff, format, programming rule audit | Scoped Ruff and format exit zero with empty stderr. Three external-boundary checker findings are manually justified: unsupported storage, open Tinygrad operations and unknown CLI modes reject explicitly; no closed internal variants or blanket suppression are involved | `resume-v4/comment-{ruff,format}.json`, `manual-audit-exceptions.json` |

Prepare-only at frames 1, 7 and 12 updates the warp output while preserving policy
queues, the preceding model output and recurrent state, matching `ModelState.run`.
Previous feature feedback comes from the source model's hidden-state slice. The
source-artifact contract hashes identify the exact small OOB artifact and current
implementation. Existing unchanged 473+473 generic HCQ numerical controls are
reused; they were not rerun for this separate staged format.

## Corrections and remaining work

`resume-v1/manifest-tests-red.json` records acceptance of a malformed kernel hash
before the admission fix. `resume-v2/tests.json` records a wrong-length test
fixture, corrected before the full test rerun. `resume-v3/capture-stderr.log` and
`input-contract-trace.json` record the first serialized fixture's missing input
contracts: suballocated frame slices were captured as constants and the arithmetic
runner ignored its wide input. The current fixture uses distinct full-frame
allocations, consumes both images and changes both frames on every prepare call.
This makes native binding or camera-input mistakes observable.

The initial unscoped pytest attempt requires unavailable Cython Params and
uninstalled inherited pytest plugins. The dedicated exporter cases were run with
the tools boundary and inherited addopts cleared; this is not an inherited
whole-repository test result. No dependency installation was performed.

Local native companion packaging, historical active/previous selection and worker
integration remain open. The source local defaults use QCOM warp and AMD queues,
so that integration also needs its genuine hybrid stage adapter or an approved
build-time AMD warp companion. A pinned `comma-run-model` `model_runtime.py`
contract is absent from retained inputs; its queue offsets are not guessed.
Trained local artifact acceptance, installed target package/AGNOS dependencies,
physical GPU execution and the user's first device comparison remain separate
pending gates. This checkpoint does not complete #154 or the full runtime port.
