Unchanged check_aarch64.py controller captures: twenty mechanically isolated
admission/frame/report controls. Original reference().build_proc_log_message and
make_process generate the procfs fixture and complete Event wire independently.
No native collector/encoder/checker generates expected values. The six ELF inputs
are inspected with actual GNU readelf using the original three flags/predicate;
these include original-admitted ELF32/BE header boundaries, not evidence those
synthetic objects execute under qemu. Source identities and fixture hashes are in
manifest.json.

Validation completed with a fresh isolated musl build of openpilot-proclogd,
actual qemu-aarch64 and three private msgq frames. The unchanged Python checker
and native checker produced identical complete reports and self-test output.
The same frozen executables passed before and after deleting the two Python
checkers, with an empty PATH and zero Python executions in the native trees.
Fresh post-deletion builds, 77 tests (ARM 11, procLog 9, CI policy 44, disk 13),
both checker feature scopes under strict Clippy, and scoped formatting passed.
Fresh actual procLog 44-cycle and ARM three-frame checks also passed; the five
executable hashes matched the before/after frozen bundle.

The older primary-cache binary failed the original PSS/nice assertions. That
retained failure is historical evidence; it is not the final fresh musl result.
The report truthfully retains device_validation=not_run: host qemu and synthetic
procfs checks do not establish device operation or complete runtime conversion.
Concise receipts are indexed in the #296 investigation record; private detailed
receipts are under .analysis/scratch/2026-10-19-aarch64-check-tools/.
