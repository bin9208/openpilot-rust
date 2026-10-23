# Xiaoge ARM emulation CI build budget (#520)

## Observed failure

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

## Focused correction and limits

Give only the emulated ARM matrix row a finite 150-minute job budget, retaining
75 minutes for x64. The measured provider compilation alone used 56 minutes,
on top of setup and tool compilation, so the shared 75-minute budget left no
room for the original bindings and required consumer checks. The new budget
provides that room; a completed ARM run must still demonstrate sufficiency.

All original commands, matrix rows, required aggregate dependencies, source and
model pins, growth admission, strict lint, failure propagation, source/native
comparisons and artifact ownership stay intact. Runtime timing and safety
limits are unchanged. This does not introduce provider substitution, caching,
hosted fallback, or a physical ARM/device acceptance claim. Other sessions'
common helper and interrupted build state are outside this correction.

Local validation is limited to YAML parsing, embedded-shell syntax and diff
review. The new job budget and full consumers must run in registered Actions
on the final candidate. Issue #520 remains open pending that result, all
required checks, normal integration and separate post-merge verification.
