# #509: operation-growth capacity admission

Issue: [#509](https://github.com/bin9208/openpilot-rust/issues/509);
[PR #500](https://github.com/bin9208/openpilot-rust/pull/500) → dev.
Preparation source: `82e1284d8602120f1e5aa834ea4cea0d2a41fb42`.
Worktree: `/home/fma1/.codex/worktrees/backlog-fixes-497/openpilot-rust`.
Status: Python policy source/tests prepared; functional results remain pending
parent review and registered Actions. No local functional tests, project
imports, fixtures, builds or dependency installation were performed.

## Actual failure and global policy

The actual [workspace job 120120936136](https://github.com/bin9208/openpilot-rust/actions/runs/39863489224/job/120120936136)
at source 82 finished failed at the reported time 22:39:17. Workspace Clippy
passed; three existing model-runtime tests then failed at
`CPU artifact emission requires 25 GiB free plus artifact growth`.
That admission failure does not establish independent model, numerical,
immutable-pointer or publication failures.

The user globally removed the fixed 25/35 GiB stop/resume policy, retaining real
capacity against estimated operation growth. Updating the shared disk-space
guard alone did not remove independent copies in native model emission and
existing Python QA/bootstrap helpers.

Parent exclusively owns `rust/crates/model-runtime/src/export.rs`.
The parent reports its native correction settled and independently reviewed:
the actual statvfs lookup and u128 free-byte arithmetic remain; admission compares
free only with growth and preserves typed free/growth failures. New private Unix
boundary controls cover exact/minus-one growth at zero, one byte, 16/256 MiB,
usize::MAX and u128::MAX, plus actual zero-growth stat and missing-path failure.
The original three failed model tests remain unchanged except the production
guard correction. This worker did not edit or independently review that file;
its actual tests and full workspace acceptance still require Actions.

## Python boundary contract

The 41 files below contain 43 reachable admission boundaries. Each previously
required **25 GiB plus G** (sometimes written as 26/27 GiB or a fractional GiB).
Each now requires **G**, preserving the original growth in the table.

The new stdlib-only `rust/tools/qa_space_policy.py::free_for_growth`
calls actual `shutil.disk_usage` on the same caller-selected path, without
ancestor fallback, cache deletion or synthetic free values. It records path,
observed free_bytes, growth_bytes, reserve_bytes=0, required_bytes=growth and
admitted=(free>=growth), then returns the observed free value. Receipts go to
stderr, leaving machine-readable stdout unchanged. Metadata errors propagate.
Negative growth is rejected explicitly; nonnegative integer byte counts remain
arbitrary precision, with no narrowing or wrap.

Each existing caller retains its assertion or original RuntimeError/OSError/
custom exception failure branch. Only capacity-related error text loses the
retired recovery instruction. Exact growth is admissible, including callers
that previously used strict greater-than. Less than growth still fails.
The V4L guard's existing stored floor is zero; other existing growth/free receipt
fields remain. No source/ABI/ELF/model/fixture assertion, ownership check,
timeout, subprocess argument, dependency pin, numerical limit or device
installer policy changes.

The recovery package's independent **25 MiB growth allowance remains**. It is
not the removed 25 GiB reserve. The webcam `--reserve-mib` spelling/default and
workflow values remain compatible; these were already the additive operation
growth component and are not silently set to zero.

## Exact changed QA paths, old reserve and preserved growth

All paths in this table are relative to the repository root. R job names refer
to `.github/workflows/rust.yml`; focused tools refers to
`.github/workflows/backlog-fixes-497.yml`. No workflow was edited by this worker.

| Changed path | Old fixed reserve | Preserved G; new fixed reserve is zero | Actual registration/call path |
| --- | --- | --- | --- |
| `rust/tools/build_athena_vision_peer.py` | 25 GiB | 1 GiB | `rust.yml` athena-runtime → check_athena_runtime vision-build |
| `rust/tools/build_locationd_oracle.py` | 25 GiB | 1 GiB per build command | estimation-runtime, direct oracle build |
| `rust/tools/build_pandad_peripheral_source.py` | 25 GiB | 100 MiB | panda-runtime → check_pandad_ci.execute, peripheral build |
| `rust/tools/build_paramsd_oracle.py` | 25 GiB | 1 GiB (both guards) | estimation-runtime, reuse-wrapper and command-loop guards |
| `rust/tools/build_plannerd_acados.py` | 25 GiB | 128 MiB | planner-runtime, normal and sanitized build |
| `rust/tools/carrot_man_stage_geos.py` | 25 GiB | sum of selected library/license bytes + 1 MiB | carrot-man-runtime, direct GEOS staging |
| `rust/tools/carrot_recovery_http_control.py` | 25 GiB | binary + helper + web asset file sizes + **25 MiB** | workspace, explicit --standalone package branch |
| `rust/tools/check_camerad_camera_lifecycle.py` | 25 GiB | 1 GiB | camera-runtime → check_camerad_ci.CameraCheck.check, camera_lifecycle and cleanup |
| `rust/tools/check_card_factory.py` | 25 GiB | 16 MiB | card-runtime → check_card_ci → factory run |
| `rust/tools/check_cweb_address.py` | 25 GiB | 1 GiB | ui-connectivity, direct native address check |
| `rust/tools/check_encoder_runtime.py` | 25 GiB | 1 GiB | encoder-runtime → check_encoder_ci runtime child |
| `rust/tools/check_encoder_v4l.py` | 25 GiB | 128 MiB per build | encoder-runtime → check_encoder_ci v4l child |
| `rust/tools/check_jpeg_sanitizers.py` | 25 GiB | 1 GiB | athena-runtime, direct sanitizer check |
| `rust/tools/check_locationd_native.py` | 25 GiB | 2 GiB | estimation-runtime, native builds |
| `rust/tools/check_paramsd_native.py` | 25 GiB | 2 GiB | estimation-runtime, native builds |
| `rust/tools/check_plannerd_ci.py` | 25 GiB | 512 MiB per child check | planner-runtime, run_checks loop |
| `rust/tools/check_plannerd_lifecycle.py` | 25 GiB | 128 MiB | planner-runtime → check_plannerd_ci lifecycle child |
| `rust/tools/check_radarcan_constructors.py` | 25 GiB | 512 MiB | radar-runtime / radar-arm → check_radarcan_ci.reference |
| `rust/tools/check_radarcan_runtime.py` | 25 GiB | 256 MiB | radar-runtime → check_radarcan_ci.ipc |
| `rust/tools/check_runtime_launcher.py` | 25 GiB | 512 MiB | startup-runtime, owned bootstrap package check |
| `rust/tools/check_ui_framework.py` | 25 GiB | 1 GiB for EGL | ui-connectivity, EGL fixture build branch |
| `rust/tools/check_ui_native_safety.py` | 25 GiB | 256 MiB | ui-runtime → check_ui_ci.UiCheck.execute native-safety |
| `rust/tools/check_ui_root.py` | 25 GiB | 512 MiB | ui-runtime → check_ui_ci.UiCheck.execute root |
| `rust/tools/check_ui_augmented.py` | 25 GiB | 512 MiB | ui-runtime → check_ui_ci.UiCheck.execute augmented |
| `rust/tools/check_usbgpu_hardware.py` | 25 GiB | 1 GiB at each of two guards | workspace, actual hardware comparison and probe build |
| `rust/tools/check_xiaoge_ci.py` | 25 GiB | 512 MiB | xiaoge-runtime, both existing architectures |
| `rust/tools/check_xiaoge_inference.py` | 25 GiB | 256 MiB | xiaoge-runtime → check_xiaoge_ci inference |
| `rust/tools/check_xiaoge_lane.py` | 25 GiB | 64 MiB | xiaoge-runtime → check_xiaoge_ci lane |
| `rust/tools/check_xiaoge_lifecycle.py` | 25 GiB | 128 MiB | xiaoge-runtime → check_xiaoge_ci lifecycle |
| `rust/tools/check_xiaoge_live.py` | 25 GiB | 128 MiB | xiaoge-runtime → check_xiaoge_ci live |
| `rust/tools/check_xiaoge_native_memory.py` | 25 GiB | 2 GiB | xiaoge-memory, direct memory check |
| `rust/tools/check_xiaoge_runtime.py` | 25 GiB | 128 MiB | xiaoge-runtime → check_xiaoge_ci http-tcp |
| `rust/tools/extract_runtime_sdk.py` | 25 GiB | 2 GiB | workspace pytest tools/tests, and focused tools → test_extract_runtime_sdk → actual CLI/debugfs |
| `rust/tools/generate_xiaoge_opencv_reference.py` | 25 GiB | 256 MiB | xiaoge-runtime → check_xiaoge_ci opencv-reference |
| `rust/tools/radarcan_prepare_assets.py` | 25 GiB | 4 × input file sizes + 64 MiB | radar-runtime, direct source-asset preparation |
| `rust/tools/stage_radarcan_runtime.py` | 25 GiB | 64 MiB | radar-runtime → check_radarcan_ci.ipc stage-runtime |
| `rust/tools/usbgpu_status_fixture.py` | 25 GiB | 128 MiB | workspace → check_usbgpu_status → hardware() |
| `rust/tools/webcam_camera_compare.py` | 25 GiB | existing --reserve-mib × MiB (default 32; workflow 512) | webcam-runtime, both existing architectures, -P direct |
| `rust/tools/webcam_capture_errors.py` | 25 GiB | existing --reserve-mib × MiB (default 32; workflow 512) | webcam-runtime, both existing architectures, -P direct |
| `rust/tools/webcam_cli_compare.py` | 25 GiB | existing --reserve-mib × MiB (default 32; workflow 512) | webcam-runtime, both existing architectures, -P direct |
| `rust/tools/webcam_worker_compare.py` | 25 GiB | existing --reserve-mib × MiB (default 32; workflow 512) | webcam-runtime, both existing architectures, -P direct |

Additional owned paths:

- `rust/tools/qa_space_policy.py`: common observation/receipt function.
- `tools/ci/tests/test_qa_space_policy.py`: controlled capacity regressions.
- `docs/naver/issue-509-space-policy.md`: this record.

Parent's native file and Dewey's settled web registration/docs/policy-test files
are outside this worker's edits. The original dirty checkout was not modified.
An initial relative-path patch was rejected by verification before application;
the successful patch used absolute paths in the assigned isolated worktree.

## Reachability and import modes

Reachability is by actual guarded entry/function, not merely importing a file.
In particular camera_lifecycle is selected dynamically by
`check_camerad_ci.CameraCheck.check`; Panda's peripheral builder is selected by
`check_pandad_ci.execute`; UI root/augmented/native-safety are selected by
`check_ui_ci.UiCheck.execute`. The SDK regression invokes the actual extractor
CLI. Existing local-import helpers and their stdout formats are preserved.
The workspace's `python -m pytest ... tools/tests` runs from its Rust working
directory and collects the SDK unittest's real CLI cases. Its USB GPU catalog
tests call `catalog` and `runtime_sources`, not the separately guarded `publish`;
that import alone does not authorize changing the catalog publication guard.

Safe-path (`python -P`) callers in workspace, webcam-runtime, planner-runtime,
encoder-runtime, radar-runtime, radar-arm and xiaoge-runtime have a job
PYTHONPATH that includes the exact checkout's rust/tools. Their drivers retain
that environment for the affected children. Radar's DBC bootstrap also retains
the existing tools path. Webcam/provider setup prepends bindings while retaining
the prior tail. These paths cover both current webcam/Xiaoge architectures;
ARM uses the separately owned local-shell/QEMU/PRoot environment.

Other affected direct CLI calls use normal script mode, where rust/tools is the
script directory. This includes carrot-man-runtime GEOS staging despite that
job's job-level PYTHONPATH containing only the repository root. Athena's child
driver explicitly includes both repository root and rust/tools. Existing SDK
tests run the extractor with normal `sys.executable script-path` mode. The accepted
new web commands retain their explicit tools PYTHONPATH; their imported
standalone Dashcam mains are not executed.

The new Actions regression launches import-only Python -P children from both
repository and Rust working directories, using the **actual current workflow
PYTHONPATH strings** for the affected safe-path jobs. It requires the loaded
helper's file identity to match this checkout. No QA main, source oracle,
fixture, third-party module or native executable is imported by that check.

## Deliberately unchanged / reported boundaries

The following three standalone mains have retired guards, but accepted web
probes only import unguarded utilities from them. Their guards are not invoked;
they were not changed:

- `rust/tools/carrot_server_dashcam_catalog.py` (source_modules import).
- `rust/tools/carrot_server_dashcam_media.py` (read_response import).
- `rust/tools/carrot_server_dashcam_upload.py` (save import).

Other retired-guard files found without a proven current registered guarded
entry are left unchanged and reported for a separately authorized scope:

- `rust/tools/build_offline_model_pipelines.py`,
  `rust/tools/check_encoder_target.py`,
  `rust/tools/check_manager_log_upload.py`,
  `rust/tools/check_card_carlog.py`,
  `rust/tools/run_xiaoge_opencv_qa.py`,
  `rust/tools/usbgpu_catalog_package.py`,
  `rust/tools/model_export/local.py`.
- `rust/tools/check_ui_native_resources.py`,
  `rust/tools/check_ui_torque_miri.py`,
  `rust/tools/check_ui_updater_signal.py`,
  `rust/tools/check_usbgpu_selection.py`.
- `rust/tools/cluster_gles_dmabuf_compare.py`,
  `rust/tools/cluster_gles_readback_errors.py`,
  `rust/tools/cluster_media_hardware_compare.py`,
  `rust/tools/cluster_pipeline_compare.py`,
  `rust/tools/cluster_vidc_compare.py`,
  `rust/tools/cluster_vidc_encoder_compare.py`.
- `rust/tools/carrot_server_auto_update_pull.py`,
  `rust/tools/carrot_server_dashcam_http/fixture.py`,
  `rust/tools/carrot_server_dashcam_metadata/fixture.py`,
  `rust/tools/carrot_server_git_state.py`,
  `rust/tools/carrot_server_git_status.py`,
  `rust/tools/carrot_server_screenrecord.py`,
  `rust/tools/carrot_server_youtube_carry.py`,
  `rust/tools/carrot_server_youtube_isolation.py`,
  `rust/tools/carrot_server_youtube_url.py`.

This is not a claim that those guards comply with the new global policy. A
future registration must account for them. Model/device catalog and update/
installation-related paths are especially not silently reclassified as host
QA. No device installer or production policy file was edited.

The aggregate baseline helper restores the entire original SDK extractor for
#292. That restoration also restores its historical 25 GiB floor. If the actual
baseline job reaches that floor before alias extraction, its capacity failure
is **not** the intended directory-alias RED. Parent must inspect the targeted
failure and keep any baseline-policy alignment explicit; this worker did not
change the aggregate baseline harness or misclassify capacity rejection as
alias-behavior evidence. The separate #509 counterexamples intentionally test
and identify that old capacity rejection.

## Regressions and parent handoff

Tests were written before the Python production patch. They execute the actual
candidate admission AST blocks, including their original failure branches and
stored V4L receipt, without importing QA modules or running their mains.
Controlled disk measurements exist only inside these unit tests. Exact-growth
admission and one-byte-below rejection exercise all 43 boundaries and preserve
their original failure type. Parameterized webcam guards additionally exercise
zero, one MiB and a very large integer growth. Shared policy controls cover zero,
one byte, 16/256 MiB, u64::MAX/u128::MAX, real zero-growth metadata and actual
missing-path rejection.

Original counterexamples use each file's pinned Git blob from exact source 82.
The test authenticates the Git blob content hash and the baseline path's object
identity, then executes the unchanged original admission AST against the same
exact-growth observation. Expected old rejection is a capacity error, not a
missing import, syntax error or failed fixture setup. No old free value is
substituted into a real CI operation. The actual guard always calls disk_usage.

Existing fast discovery already registers the new stdlib test through
`python -m unittest discover -s tools/ci/tests -v`, with full Git history.
Dedicated parent command in registered Actions:

```sh
python -m unittest tools.ci.tests.test_qa_space_policy -v
```

No test was executed locally. Parent owns independent review, original three
model-test execution, full workspace and affected job results on the next exact
candidate, normal dev merge and separate post-merge verification. All necessary
Actions execution stays on bin-actions; existing ARM checks remain accurately
labelled QEMU/PRoot emulation. Preserve unchanged successful checks and other
jobs' data. Source preparation, syntax and diff review do not establish runtime,
device, physical ARM or performance acceptance.

Local static results: AST syntax parsed all 43 owned Python files; AST inspection
counts 43 caller observations. Whitespace review covered all 44 owned paths and
scoped `git diff --check` passed. No project module, guard or test was executed.

## Parent alignment of the #292 behavioral counterfactual

After the Python handoff, the parent updated `issue_497_baseline.py` separately.
The #292 counterfactual still restores the actual original extractor from
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, blob
`007cc4f723139d3a15fd986a76fb5102f3dc9131`. Before its real ext4 test, it changes
only the original capacity comparison from 27 GiB to the preserved 2 GiB growth,
and the associated diagnostic. The original filesystem measurement, image
size/digest validation and defective directory walker remain. It does not fake
available bytes or apply the candidate's alias repair to the counterfactual.
The existing `finally` restoration still restores the complete candidate file.

`issue_292-source.json` records both actual original and exercised SHA-256 values,
the baseline commit/path and the explicit operational policy adjustment. This
is an old-behavior counterfactual under the current capacity policy, rather than
a claim that the entire exercised file is byte-identical to the old file.
The separate #509 capacity controls retain the completely unchanged old guards.

The behavioral RED must report one assertion failure whose subprocess error is
`missing image file /usr/include/real:` with `Type: directory`. Capacity,
import/syntax and ENOSPC setup failures are rejected. The new fast-discovered
`test_sdk_baseline_attribution.py` authenticates the original blob, checks that
all other AST nodes are identical, exercises both actual capacity guards at
exact growth/one byte below, and checks accepted/rejected failure signatures.
Those controlled observations are unit-only; the ext4/debugfs proof still runs
against actual storage in the focused tools job. Local validation is AST/diff
only. The new source, test and counterfactual execution remain pending Actions.
