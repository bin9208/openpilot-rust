# Canonical ARM emulation CI build budgets (#520)

## Observed Xiaoge failure

PR #500 source `10919538d31b04d28d9a23b383d925441434d579` ran the
unchanged ARM commands on registered `bin-actions` using pinned QEMU/PRoot.
[Job 120220822647](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/job/120220822647)
started at 07:25:58 UTC on 2026-10-23. Its check-run annotation explicitly
reports `The job has exceeded the maximum execution time of 1h15m0s`.
GitHub automatically cancelled it at 08:41:47 UTC; this was not a manual
cancellation or a compiler-error result.

The provider/bindings step started at 07:34:42 UTC. After compiling the Rust
build tool, `build-xiaoge-opencv` ran at 07:42:10 and produced a `status:PASS`
receipt at 08:38:10 for pinned OpenCV source
`fe38fc608f6acb8b68953438a62305d8318f4fcd`, including the actual core, dnn and
imgproc libraries and their source/seal hashes. This external provider build
receipt does not establish ABI or complete consumer acceptance.

The original Params and VisionIPC Python bindings were still compiling when
the job budget expired. The last C++ messages at 08:41:22 and 08:41:27 were
signedness warnings, not compiler errors. The job did not reach the provider
Python identity comparison, all native Xiaoge targets, strict Clippy, or the
source pixels/model/socket/live-IPC/lifecycle comparisons. Those remain
unverified for this ARM candidate. The existing always steps uploaded evidence
and cleaned only this job's owned temporary directory.

The uploaded `rust-xiaoge-ubuntu-24.04-arm-evidence` artifact `12488703852`
contains the provider command/exit/source/seal receipts and completed Params
binding provenance. Its 390,789-byte ZIP matches the Actions API SHA-256
`a1d6a37c8316b12494234cb8fd5bd7448e91c7ea4b7fd0ea58c92ff599554f9f`.
The QEMU execution and Params receipts identify PR merge source
`03a6bc0cfdcb0ff6cb2c2dd43a0ac84267da5201` (109 into dev), while the execution
receipt explicitly reports `physical_arm_cpu:false`. No complete provider
Python or native-consumer result is present. The separate common-helper
rootfs receipt typo tracked in #515 is not silently corrected here.

## Observed planner failure

The first correction gave only Xiaoge ARM 150 minutes. The subsequent source
`36b3c1b28b26242f65c39bd29beb413a859b74bd` exposed the same execution-platform
problem in
[planner ARM job 120333191116](https://github.com/bin9208/openpilot-rust/actions/runs/39928861901/job/120333191116).
It started at 09:32:58 UTC; its annotation reports
`The job has exceeded the maximum execution time of 45m0s`, with automatic
cancellation completed at 10:18:39 UTC.

Environment setup, pinned solvers and original bindings consumed about
31 minutes before native checks began at 10:03:40. Cargo tests finished at
10:14:54, with 29 tests passed and zero failures/ignored across all targets;
binaries finished at 10:15:56. Strict Clippy was still checking dependencies
when cancellation interrupted it at 10:18:27. No compiler error or failed test
preceded the cancellation. Source comparisons and sanitizer checks had not
started; neither partial compilation nor the passing units establish them.

Artifact `12493424820` is a 21,496,531-byte ZIP authenticated against the API
SHA-256 `d5174f45edbd650af0287d2bf53acd45fff2139421f6aa657aa30805cc15cf86`.
Execution and Params receipts identify PR merge
`3b72621de9c0e2bce6cf1b5ff15cf37a973f6e98` (36b into dev), an aarch64 QEMU
guest on x64, and `physical_arm_cpu:false`. All observed growth admissions
passed; this failure is separate from the earlier capacity incident.

## Shared execution-platform correction and limits

Use a consistent finite 150-minute job allowance for the eight canonical
QEMU/PRoot ARM jobs. The two measured workloads show why inheriting the native
CPU job budgets leaves required work unexecuted. They do not establish a
specific slowdown multiplier or the final completion time of every job.

| Job | Retained x64 allowance | QEMU ARM allowance |
| --- | ---: | ---: |
| `webcam-runtime` | 35 minutes | 150 minutes |
| `planner-runtime` | 45 minutes | 150 minutes |
| `carrot-man-runtime` | 25 minutes | 150 minutes |
| `webrtc-runtime` | 45 minutes | 150 minutes |
| `xiaoge-runtime` | 75 minutes | 150 minutes |
| `radar-arm` | Separate ARM-only job | 150 minutes |
| `radard-runtime` | 45 minutes | 150 minutes |
| `carrot-navi-arm` | Separate ARM-only job | 150 minutes |

Xiaoge already had the ARM allowance, so this follow-up changes seven job
fields and keeps the saved CarrotMan template and its existing policy
assertion consistent. Generic host cross-build `arm64` remains at 120 minutes,
workspace at 120, and pure planner Miri at 45. Focused host jobs are unchanged.
This is a provisional upper bound; any further expiry requires examination of
its actual phase rather than an automatic increase.

All original commands, matrix rows, required aggregate dependencies, source and
model pins, growth admission, strict lint, failure propagation, source/native
comparisons and artifact ownership stay intact. Runtime timing and safety
limits are unchanged. This does not introduce provider substitution, caching,
hosted fallback, or a physical ARM/device acceptance claim. Other sessions'
common helper and interrupted build state are outside this correction.

Local validation is limited to YAML parsing, embedded-shell syntax and diff
review. The new job allowances and full consumers must run in registered Actions
on the final candidate. Issue #520 remains open pending that result, all
required checks, normal integration and separate post-merge verification.
