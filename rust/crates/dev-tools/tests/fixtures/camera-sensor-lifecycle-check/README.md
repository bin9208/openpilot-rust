# Independent original sensor lifecycle comparator captures

Expected values come from unchanged check_camerad_sensor_lifecycle.py, pinned by
SHA256 and CPython version. The 62 ordered cases cover three ports, both enable
states, probe failures, five operation/skip boundaries and retries through 100.

The process fixture records 37 independently executed original-main scenarios:
all original case names/argv/environment order, 124 child invocations per
completed run, exact receipts, progress, traces, stdout, errors and filesystem
effects. Trace JSON is parsed before run.json is written (different from the
kernel checker). This checker compares CLOEXEC flags literally; kernel-only
normalization must not be applied. The native-cloexec case pins that difference.
Six CLI captures and a real original raw qemu/output-path run preserve inherited
stdin, percent-encoded colon case names, 124 children and complete case results.

The retained live report passed all 62 actual C++/Rust cases. All three current
binaries were rehashed against that receipt before reuse; this is original
existing evidence, not a new full execution. Native live verification now passes all 62 cases with PATH disabled and the
unchanged SDK sanitizer mounted at the original path; its report is byte-identical.

The retired Python checker formerly imported kernel digest/disable_core.
ISP normalize is also imported by camera lifecycle; camera cleanup audit is
imported by camerad_runtime_check. Those helpers cannot be deleted merely by
retiring an individual CLI.

Capture scripts are in ignored
.analysis/scratch/2026-10-18-camera-sensor-lifecycle-check/. New native tests/bin
observed missing-module/bin RED in camera-kernel-check/sensor-missing-red.log.
The native implementation is source-ready; compilation, regression gates and
the current-source live proof remain pending the parent's compiler slot.
No completed retirement or device acceptance is claimed.
