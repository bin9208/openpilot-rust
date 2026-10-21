# Panda sanitizer and execution audit separation (#440)

Actual8e8b1e8f full Panda job118974708273 failed in the sourceabsence trace:
LeakSanitizer reports that it cannot operate under ptrace. The fresh retirement
proof atafe446e3 likewise ran its original successfully but both traced native
lanes exited1. These failures are QA execution configuration; no runtime safety
or vehicle defect is established.

All four requests and full transcripts/reports continue through fresh sanitized
original and native controllers without ptrace. A separate plain original probe
is built solely for strace. Its manifest must have the same21pinned source files
as the sanitizer build, with explicitly opposite sanitize flags. Complete plain
traced outputs must match the original/sanitized observations. Both native roots
use the same checker/candidate and each selected source ELF; successful exact
exec vectors, manifest/executable seals, input requests, original reports and
sourceabsence remain mandatory. LeakSanitizer/ASan/UBSan are never disabled.
The existing full Panda source/original suite and old finalization behavioral
RED/fixed regressions remain. Python is retained pending this new cloud proof.
Local checks are formatting, syntax and diff inspection only. No new successful
functional or sanitizer claim is made before Actions completes.
