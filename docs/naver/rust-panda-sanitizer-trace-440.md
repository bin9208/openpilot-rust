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
RED/fixed regressions remain. Python remained present during the new cloud proof; post-deletion caller integration is the next gate.
Local checks are formatting, syntax and diff inspection only. No new successful
functional or sanitizer claim is made before Actions completes.

## Accepted source20a proof (2026-10-21)

[Run39529140105/job118990740395](https://github.com/bin9208/openpilot-rust/actions/runs/39529140105/job/118990740395) succeeded on checkout `20a8f52e83d85c7c5113c5484179b0c016a034eb`; diagnostic workflow commit `650c61e194307696b38a5cd5b13132d508e95484` is a separate identity. Artifact12314188013 is3,469,683bytes, SHA256 `3153e98dd6ff053928cff93c4549bc105217632d7a5aa103a440147cfe1102d0`; `source-commit.txt` pins the checkout.

Parent independently compared original and four native lane reports, all six complete stdout/stderr streams per native lane and nine parsed requests per lane (one CarParams and four each source/native). Only validated owned Params root names differ. Original report SHA256 remains `c06b958690b1fe277ffba4ff99f9f0c9f15357ca23b9abf88fe925409c271712`. Sanitized original/present/absent exit0; both plain traced lanes match those observations and each records exactly four successful exec vectors over three sealed native executables. Both builds bind the same21source hashes. The old finalization behavioral RED and current finalization, reference, absence, retirement and strict Clippy gates all succeeded.

Companion current-source/disk policy run39529140126 also succeeded. This repairs the ptrace validation conflict without changing Panda runtime or weakening sanitizer coverage. Remaining: remove the original checker only with maintained sealed-original callers and verify the actual deleted-source checkout, then latest complete PR checks. Device behavior remains user-owned.
