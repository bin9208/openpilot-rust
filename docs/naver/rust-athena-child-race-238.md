# Athena camera fixture child inspection: issue 238

Camera integration PR #236 at `094a4fafe` failed its
[Athena job](https://github.com/bin9208/openpilot-rust/actions/runs/37779176489/job/113317615204)
before the camera assertions. The fixture enumerated a child and then its
`/proc/9463/exe` resolution raised FileNotFoundError. The resulting `finally`
block stopped the owned snapshot/camera process; those cleanup messages do not
establish a camera runtime failure. The job used Python 3.12.15.

The discovery helper now skips only FileNotFoundError while resolving an
enumerated child's executable. Parent enumeration, expected-executable
resolution, PermissionError and other I/O failures still propagate. The original
six-second discovery deadline, process/cleanup deadlines, exact JPEG comparison,
Params checks, reaping and already-running-camera assertions are retained.

Six standard-library controls pass on local Python 3.12.3. The race test injects
the precise observed resolution exception for an already-reaped owned child,
then identifies a following live child. A separate live-child check uses real
Linux process enumeration/executable identity and observes cleanup through a
pidfd. AST comparison confirms the original main body is unchanged except for
the discovery callback. The existing Athena CI job now runs these focused tests
before its full runtime and JPEG sanitizer checks.

Evidence is in `.omo/evidence/238-athena-fixture-child/` in the issue checkout.
The original artifact is
[11551459242](https://github.com/bin9208/openpilot-rust/actions/runs/37779176489/artifacts/11551459242).
No matching retained snapshot/IPC/vision binary set was available locally, so
the corrected full lifecycle and sanitizer checks await the updated integration
head's hosted job. No local full-runtime pass or device test is claimed.
