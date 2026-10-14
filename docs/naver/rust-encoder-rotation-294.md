# Encoder rotation investigation (#294)

Issue: [#294](https://github.com/bin9208/openpilot-rust/issues/294).
Draft PR: [#295](https://github.com/bin9208/openpilot-rust/pull/295).
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
path handles the header and EOS cases. The EOS fixture now uses the timestamp
observed in the approved physical probe below; header/input negative values
remain synthetic coverage.

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
failure, and `v4l-physical-eos/` contains the corrected comparison including the
observed EOS timeval; `v4l-physical-final/` is the final full-matrix run after
restoring the fixed build. `physical-eos-red/` replays that exact timeval against the
old Rust fixture (SIGABRT with the conversion error), source (exit 0) and fixed
Rust (exit 0). Source and fixed Rust publish identical 24-frame traces. The first
host build lacked the explicit existing libyuv archive directory; setting
`ENCODER_LIBYUV_LIB` to the prepared native SDK fixed the build environment
without installing dependencies. Build space checks retain more than 394 GiB
free and use the coordinated cache with incremental compilation disabled.

The user explicitly approved the isolated hardware probe. With offroad state
and absence of a running encoder checked immediately beforehand, the independent
source-based executable encoded two segments of eight synthetic frames using
owned ION inputs. It uses no msgq, camera capture, settings or vehicle-control
interfaces. Its dependencies were checked with the verified AGNOS loader/libc.
The run exited 0 within the 20-second timeout bound and published all 16 frames.
Both EOS capture buffers had flags `0x02004000`, zero bytes/offset and
`timeval { tv_sec: -1, tv_usec: 999999 }`, representing -1 microsecond.
The source handled both EOS packets. The host fixture reproduces this timestamp;
its other unchanged flags are scripted rather than a full hardware capture.
Probe outputs were copied privately, the temporary device directory removed,
and absence of the probe process and offroad state rechecked.

The probe establishes a hardware-backed failure mechanism: the actual EOS value
triggers the old Rust conversion failure, and the corrected host boundary handles
it. The corrected full Rust runtime has not been installed or driven, so resolution
of the reported driving crashes remains unverified. No runtime package has been
installed. Exact-SHA CI and corrected-runtime target validation remain pending;
#294 and full runtime #1 stay open. No CPU saving is claimed from this parity fix.

The implementation commit is `f017fbf132290290fd089e676a6f90ee6a14801f`.
Its [fast checks](https://github.com/bin9208/openpilot-rust/actions/runs/38530097594)
and [user docs](https://github.com/bin9208/openpilot-rust/actions/runs/38530097578)
passed. The [encoder CI job](https://github.com/bin9208/openpilot-rust/actions/runs/38530097704/job/115739653300)
passed source build, codecs, all 32 V4L cases and actual host runtime comparison,
then failed its final completeness check because `check_encoder_ci.py` still
expected 28 cases. Both the required count and summary now require 32; no check
was relaxed. [Integration](https://github.com/bin9208/openpilot-rust/actions/runs/38530097787)
was still running when recorded. Follow-up commits need their own exact-head
results; these links do not establish success for another SHA.

Docs-Not-Needed: internal video-codec source-parity correction; no setting,
video quality/rate or intended user behavior changes.
