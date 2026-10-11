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

The continuation below adds native companion packaging, historical selection and
worker integration. A pinned `comma-run-model` `model_runtime.py`
contract is absent from retained inputs; its queue offsets are not guessed.
Trained local artifact acceptance, installed target package/AGNOS dependencies,
physical GPU execution and the user's first device comparison remain separate
pending gates. This checkpoint does not complete #154 or the full runtime port.

## Local package and worker continuation (2026-10-12)

`build_usbgpu_local.py` explicitly converts trusted captured source artifacts at
build time. It derives packed inputs and hidden-state feedback from the source
metadata, exports both camera sizes, and publishes an immutable hashed generation
with verified firmware, probe and licensing companions. The native worker does
not invoke Python. AMD warp/policy and separate QCOM warp/AMD policy packages use
the source stages. Hybrid admission checks the exact bridge sizes and native QCOM
kernel argument contracts before opening a GPU.

Local input buffers follow `get_nv12_info`: 2,428,928 bytes at 1344x760 and
4,804,608 bytes at 1928x1208, including source allocation padding. Each comparison
receipt records Y/UV offsets, strides, plane lengths and the received buffer
length. The generic worker retains its existing 1,622,016/3,735,552 plane-byte
contract. A plane-only local frame is rejected.

The local adapter owns queue state and hidden-state feedback. Its prepare-only
request warps the current images and preserves policy queues and the preceding
published output. The generic fused adapter still produces its full policy
output for a prepare request. Nonfinite local output fails before feedback or
publication; subsequent work on that runtime is rejected.

Native readiness now follows active local selection and previous local fallback;
an active precompiled-only selection blocks that fallback. It requires verified
native companions, beyond the original Python chunk marker. Status keeps active
compile-pending separate from availability of a previous compiled model. Repeated
status polls reuse at most two unchanged admitted generations using pre/post
inode, size, mtime, ctime and directory checks. Worker startup always performs full
validation. The shared path resolver preserves an empty cache override as a
fallback to the normal source location.

Receipts below are under `.omo/evidence/154-runtime-resume/local-worker/`.

| Scenario and invocation | Actual observable | Artifact |
| --- | --- | --- |
| Source `usbgpu_local_source.run(capture=False, camera=...)`; `check_usbgpu_local_worker.py --camera ...` | Both sizes execute 15 owned frames, including three prepare-only calls; 180 model/warp/queue observations are byte-equal and finite. The 1928 run additionally rejects nonfinite inference and subsequent work. 261+281 actual emulator kernels include that failure control | `source-full-invocation.json`, `source-1928.json`, `native-full-invocation.json`, `native-1928.json`, `full-frame*/comparison.json` |
| Trusted source capture and hybrid publication through pinned ARM QCOM compiler | Actual small source warp kernels and AMD policy export for both sizes; native worker admits both QCOM/AMD bridge contracts. This is structural admission, **not QCOM GPU execution or numerical acceptance** | `hybrid-publish-v5.json`, `structural-final.json` |
| Worker `--check-artifacts` against the genuine converted package | 15 positive/negative controls include corrupt firmware/kernel/weights/probe/PKL, scalar/hidden/camera/path errors and real chunk precedence with an empty tail | `structural-controls.json` |
| `local_selection` with captured syscalls and source helper comparison | Repeated status performs zero package/PKL opens; full worker validation opens companions. Changed generation and changed same-size kernel reject; historical local selection agrees in five source/native cases | `selection-admission.json`, `admission-full.strace`, `source-selection.json` |
| `worker_client_local` against owned protocol worker | Prepare-only omits output, preserves previous bytes, full local input succeeds, plane-only input fails and later work is rejected | `client-protocol.json`, `client-inputs.jsonl` |
| Shared path resolver against unchanged `model_cache_dir` source body | Absent, empty and explicit cache overrides agree | `runtime-paths.json` |
| Original `read_state` and native selection with malformed active and valid previous; worker admission through a symlinked package prefix | Both selectors invalidate the complete malformed state. The owned symlink control reproduces acceptance before the fix and rejects it after prefix validation | `malformed-active.json`, `malformed-active-invocation.json`, `package-prefix-symlink-{red,green}.json` |
| Native `--boot` with no USB presence and an owned closed catalog port | Converted local package selects successfully; missing companions and active precompiled-only format retain the internal path | `boot-local.json` |
| Scoped native/Python checks | Nine focused Rust cases and two exporter cases pass; all-target strict Clippy, driving-model caller check, Ruff/format pass. Four pure Rust boundary cases pass both Miri alias models; existing unrelated nightly manifest warnings remain explicit | `tests-final.json`, `python-tests-final.json`, `strict-final.json`, `drivingd-check-final.json`, `ruff-final.json`, `format-final.json`, `miri-{stacked,tree}.json` |

The Python rule checker reports three external open-set match boundaries
(captured storage, Tinygrad operations and build backends). They have explicit
unsupported-input errors and manually justified exceptions in
`manual-audit-exceptions.json`; Ruff accepts their ordinary explanatory comments.

The small model and synthetic ONNX identity remain engineering fixtures. The
trained local PKL is unavailable. Existing generic numerical controls are reused;
no unchanged generic kernel replay, device access, NAS modification, production
selection change, CPU benefit or complete-runtime claim is included.
