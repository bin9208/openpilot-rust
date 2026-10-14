# Encoder rotation investigation (#294)

Issue: [#294](https://github.com/bin9208/openpilot-rust/issues/294).
Parent inspection: [#293](rust-drive-runtime-audit-293.md).
Source base: `f3446f65e1cbd894aba1aac7e724c12e6d89ff61`;
the installed and driven package is `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`.
The changed encoder source is identical between those revisions before this fix.

The full drive shows 29 SIGABRT restarts of main encoderd during minute rotations,
with video gaps and logger segment resynchronization. It does not contain the
exact userspace abort diagnostic. Kernel VIDC teardown errors do not by
themselves identify the triggering userspace operation.

## Reproduced source-parity defect

Original `dequeue_buffer` returns a signed `timeval`. The source worker only
matches its timestamp for actual encoded frames. It does not check timestamps
on codec headers, EOS or returned input slots. Rust previously converted every
dequeued timestamp component to `u64`, causing SIGABRT on an irrelevant negative
component before the worker could inspect packet flags or queue direction.

The existing actual-source/native V4L boundary comparison now injects negative
timestamps separately for those three unused cases. Before implementation,
the source completes both segments while Rust aborts with `out of range integral
type conversion attempted` on the returned-input case. The same implementation
path handles the header and EOS cases. These are synthetic driver inputs, not
yet observed physical EOS values.

The fix retains dequeued timestamps as signed `i128` microseconds, widening
before multiplication. This represents all signed 64-bit timeval components
without overflow. Actual encoded-frame timestamps must still exactly match the
submitted EOF microseconds. Offset, size, queue ownership, buffer retention,
rotation, scheduling and codec configuration checks remain unchanged.

## Verification

The full original/native driver-boundary matrix passes 32 cases, including two
rotations, ordered frame publications, headers/EOS, slot-order restoration and
ION allocation/free counts. The new three unused-timestamp cases pass; a new
negative actual-frame timestamp still fails the exact synchronization assertion
in both implementations. Existing wrong timestamp, offset, queue, capability
and format failures retain their required failure outcomes.

The native encoder's six Rust tests and the 13 focused Python harness tests
pass. Package-scoped formatting, Clippy with warnings denied, Ruff and
`git diff --check` pass. The Python harness invocation uses
`--confcutdir=rust/tools/tests` to avoid the unrelated root conftest's missing
host `params_pyx` extension. Independent read-only review found no actionable
issues in the four code/fixture files; it did not establish hardware causality.

Private local receipts are under
`.analysis/scratch/2026-10-14-drive-audit/`: `v4l-red/` captures the pre-fix
failure, and `v4l-final/` contains the complete corrected comparison. The first
host build lacked the explicit existing libyuv archive directory; setting
`ENCODER_LIBYUV_LIB` to the prepared native SDK fixed the build environment
without installing dependencies. Build space checks retain more than 394 GiB
free and use the coordinated cache with incremental compilation disabled.

The independent source-based hardware probe uses owned ION inputs and no msgq,
camera capture, settings or vehicle-control interfaces. Its two segments of
eight synthetic frames are ready for a time-limited offroad run. Its ELF closure
loads with the verified AGNOS loader and libc under QEMU. It has not run on the
device; approval is pending under the repository's device-test restriction.

Do not claim this fixes the observed drive abort until the physical boundary is
localized and a corrected source-matched runtime passes rotation. No runtime
package has been installed. Exact-SHA CI and target validation remain pending;
#294 and full runtime #1 stay open. No CPU saving is claimed from this parity fix.

Docs-Not-Needed: internal video-codec source-parity correction; no setting,
video quality/rate or intended user behavior changes.
