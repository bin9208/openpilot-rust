# Full ARM/AGNOS runner comparison (#498)

The owner requested paired full builds on GitHub-hosted `ubuntu-24.04` and
registered Actions runner `bin-actions`, then instructed ongoing work chats to use
the fastest validated runner. The [focused comparison (#496)](runner-comparison-496.md)
already passed three rounds on each runner. [Issue #498](https://github.com/bin9208/openpilot-rust/issues/498)
records the results and the owner's final decision to use bin-actions. The owner
ended the unfinished hosted generic comparison after reporting recent hosted
runner 404 problems; that report is not a diagnosed cause of this run's delay.

## Results

| Workload | GitHub-hosted | bin-actions | Observed result |
| --- | ---: | ---: | --- |
| Complete AGNOS job, including setup/upload/cleanup | 44m 47s | 35m 47s | Both succeeded; bin-actions 20.1% shorter |
| AGNOS runtime/OpenCV/ABI/seal/offline installation stage | 36m 51.520s | 27m 52.356s | Both succeeded; bin-actions 24.4% shorter |
| Complete generic GNU workspace / musl probe ARM job | Ended by owner during setup | 55m 41s | bin-actions succeeded; no complete paired generic time |
| QEMU ARM offline installation command interval | 103.238s | 75.393s | Both succeeded; local interval 27.0% shorter |

The complete AGNOS jobs are [GitHub-hosted job 119987116428](https://github.com/bin9208/openpilot-rust/actions/runs/39825360851/job/119987116428)
and [bin-actions job 119987118119](https://github.com/bin9208/openpilot-rust/actions/runs/39825360851/job/119987118119).
Both have conclusion **success**. Their parent run was subsequently cancelled to
stop an incorrectly configured, separate generic comparison; that cancellation
does not invalidate the finished AGNOS jobs.

Corrected generic comparison: [run 39832976197](https://github.com/bin9208/openpilot-rust/actions/runs/39832976197),
[hosted job 120013728693](https://github.com/bin9208/openpilot-rust/actions/runs/39832976197/job/120013728693)
and [bin-actions job 120013728323](https://github.com/bin9208/openpilot-rust/actions/runs/39832976197/job/120013728323).
The hosted generic job finished **cancelled** on the owner's instruction to
use the local runner, after 4414s (73m 34s) elapsed during setup. That is an
interrupted preparation duration, not a complete build time or a compiler failure.

The corrected bin-actions job completed successfully in 3341s (55m 41s),
including all 15 measured build/setup stages, upload and owned cleanup. The GNU
workspace stage took 1176.028s and the musl static C3X probe stage 1266.828s.
Every measured stage exited zero; recorded cgroup oom/oom_kill counters remained
zero. The hosted job was still provisioning dependencies when cancellation was
requested. Completed logs show long package downloads and `Ign:` retries from
`azure.archive.ubuntu.com` during APT installation. They do not establish an
HTTP 404 runner fault; a branch name containing 404 is not an HTTP error. Its
complete build time and compiler outcome are unmeasured. No retry was started.

## Comparable source and build scope

- Runtime baseline: dev-md `dc95138076836ea3a5c1b6c27262be3f3b912f71`.
- Exact paired AGNOS comparison source/controller:
  `b538bb7d37eb3cd756ff1fe7c52fd23507142605`.
- Exact corrected paired generic source/controller:
  `2ff834da04e2acaad3d63211c2b76e287679eeaa`.
- Both runners use Rust 1.94.0, two Cargo build jobs, no incremental compilation,
  and isolated fresh Cargo, target and provider directories. System packages and
  runner tool installations can be retained on the persistent host; setup time
  is reported separately rather than described as a clean machine comparison.
- Comparison changes are limited to instrumentation, the workflow, documentation,
  and explicit build-side self-hosted/disk policy exceptions. Runtime behavior
  is unchanged. Corrected generic source differs from the AGNOS source only in
  the comparison workflow, restoring original Eigen exports and adding diagnostic
  retention. No successful AGNOS build was repeated for those workflow changes.
- AGNOS builds 66 project packages / 88 configured executable names with a fresh
  OpenCV provider against genuine `19.8-carrot-bt1`, audits ELF closure, seals the
  package and performs offline ARM/QEMU installation and installed-launcher
  package verification. Unchanged non-OpenCV external companions, firmware and
  models use verified pinned baseline assets, following the existing procedure;
  they are not all rebuilt from source in this comparison.
- Generic ARM retains the original full GNU workspace and native Card,
  selfdrived, navigation, Panda, camera, encoder and UI build/inspection steps,
  including the original VisionIPC ION boundary and musl static C3X probe checks.
- The hosted runner has four logical AMD EPYC 7763 CPUs and about 16 GiB RAM.
  `bin-actions` exposes four logical Intel i9-13900 CPUs, 4 GiB RAM and 2 GiB swap.
  This compares the actual registered environments, not CPU models in isolation.

## AGNOS stage evidence

| Measured stage | Hosted seconds | bin-actions seconds |
| --- | ---: | ---: |
| Dependencies/toolchain | 43.985 | 16.106 |
| Focused source policy | 1.227 | 1.039 |
| Genuine sysroot and verified baseline preparation | 123.891 | 200.279 |
| Host release tools from fresh target | 264.963 | 205.432 |
| Complete runtime/OpenCV/ABI/seal/offline installation | 2211.520 | 1672.356 |

All listed stages exited zero on both runners. Network/preparation was slower
on bin-actions for the sysroot stage even though its complete job was faster.
Timestamped top-level command boundaries recorded OpenCV native compilation at
376.029s hosted / 289.406s local and the full Rust runtime build at 1579.709s /
1182.722s. These are intervals between controller log markers, not independent
isolated compiler benchmarks.

Both `elf.json` receipts report `compatible: true`, with 144 objects and 115
roots. Both `offline-install.json` receipts bind the exact AGNOS source above,
report `installer_exit: 0`, `installed_agnos_launcher_exit: 0`, and
`device_execution: false`. Runner provenance records the actual hosted or
self-hosted environment. The local full-stage cgroup `oom` and `oom_kill`
counters were zero before and after the build; no out-of-memory or disk-full
failure was observed in these successful jobs. This is one full cold-target
trial per runner, not evidence of long-term reliability or physical-device boot.

## Invalid initial generic attempt

The comparison controller initially installed pinned Eigen but omitted the
original `LOCATIOND_EIGEN_INCLUDE` and `PARAMSD_EIGEN_INCLUDE` exports. Local
[job 120009804015](https://github.com/bin9208/openpilot-rust/actions/runs/39825360851/job/120009804015)
failed at `openpilot-locationd` with `eigen3/Eigen/Dense: No such file or
directory`. The corresponding hosted generic job was cancelled to avoid
measuring the same incomplete setup. This is a comparison setup defect, not an
observed runner resource or stability failure. The first attempt's logs remain
part of the evidence.

The corrected generic-only workflow restores both exports exactly and retains
additional redirected FFmpeg/encoder/OpenCV diagnostics. The completed AGNOS
jobs were preserved. Independent GPT-6.1 Sol review found the diagnostic
retention gap, but did not detect the Eigen omission; review
is not a substitute for execution evidence.

## Runner selection and work-chat instructions

The owner's final instruction is to use `bin-actions` for trusted x64 checks,
generic ARM cross-builds and AGNOS cross-builds, with no automatic fallback to
the standard GitHub-hosted x64 runner. The unfinished hosted generic comparison
was stopped and its follow-up automation deleted. Existing unrelated running
validations and unchanged successful checks are preserved.

Actual ARM CPU execution stays on GitHub ARM runners while the registered local
runner is X64. QEMU ARM offline installation succeeded on both runners. The
103.238s hosted / 75.393s local intervals run from the logged installer command
to the subsequent installed-launcher verification command. They include hashing,
filesystem work and controller overhead; they are not an isolated ARM instruction
benchmark or a full runtime performance comparison. Installed-launcher package
verification also passed on both; physical-device execution was false.

This final policy was sent to the ongoing CPU optimization (#422), tool
conversion (#296), Jetlink (#487) and issue-maintenance chats, with instructions
to persist it and apply it to their next necessary workflow change. The fixed
25/35 GiB stop/resume instruction was removed from AGENTS.md at the owner's
request. No unfinished hosted outcome is a prerequisite for using bin-actions.

No release publication, protected-branch merge or physical-device operation was
performed by the comparison. Local work was limited to syntax/format/diff review
and read-only collection/analysis of Actions evidence. Any release work by other
chats retains its separate source-specific authorization and validation gates.
