# Independent original camera kernel comparator captures

Expectations come from unchanged rust/tools/check_camerad_kernel.py; its SHA256
and CPython version are recorded. Generation preserves all 334 ordered scenarios,
25 first-per-mode baseline selections, retry/fault/environment matrices and 14
trace-normalization cases, including huge integer flags, booleans and failures.

The live original report and per-lane normalized trace digests were independently
recaptured from the earlier full current-source C++ versus Rust kernel_operations
run. All 334 cases passed. Before recapture, all three binaries (source, native and
preload fixture) were rehashed and checked against the original report, and each
stored stderr digest was checked against original bytes. This is reuse of
original live evidence, not a newly executed full run. The native replacement now independently passes all 334 cases and its complete
report is byte-identical. The existing LLVM18 SDK is mounted at the original
absolute sanitizer path inside a private namespace; comparator code/settings
are unchanged and PATH is disabled.

The 39 controlled original-main cases pin subprocess argv, inherited/overlaid
environment ordering, qemu/sysroot behavior, timeout and child-only core-limit
hook, sanitizer checks, trace cleanup/parsing/normalization, stdout/error decoding,
short-circuit comparisons, exact receipts/progress and filesystem error phases.
Six real original CLI cases cover argument classes. Controlled cases use original
Python code with only subprocess results and scenario selection controlled; no
candidate code contributes expected values.

The original digest()/disable_core() helpers are still imported by sensor, ISP
and camera lifecycle checkers. CLI conversion cannot claim complete retirement of
that helper closure. Child core limits must apply only to each spawned child;
the parent hard or soft limit must never be lowered.

Capture drivers are retained in ignored
.analysis/scratch/2026-10-18-camera-kernel-check/. The intended missing-module/bin RED was observed before production code.
Native focused tests, camera routing checks, strict Clippy and the actual
replacement comparison pass. A separate actual inherited-stdin counterexample
first returned EOF natively versus token originally; preserving child stdin
resolves it without changing the parent process. No hardware/device acceptance
is claimed.
