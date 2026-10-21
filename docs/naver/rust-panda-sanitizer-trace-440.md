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

Parent independently compared original and four native lane reports, all six complete stdout/stderr streams per native lane and nine parsed requests per lane (one CarParams and four each source/native). Only validated owned Params root names differ. Original report SHA256 remains `c06b958690b1fe277ffba4ff99f9f0c9f15357ca23b9abf88fe925409c271712`. Sanitized original/present/absent exit0; both plain traced lanes match those observations and each records exactly four successful exec vectors over three sealed native executables. Both builds bind the same21source hashes. The old finalization behavioral RED and current checker, finalization, absence, retirement and strict Clippy gates all succeeded.

Companion current-source/disk policy run39529140126 also succeeded. This repairs the ptrace validation conflict without changing Panda runtime or weakening sanitizer coverage. At this point removal of the original checker and actual deleted-source checkout verification remained. Device behavior remains user-owned.

## Actual deleted-source checkout (2026-10-21)

[Run39532343946/job119000942744](https://github.com/bin9208/openpilot-rust/actions/runs/39532343946/job/119000942744) succeeded. The diagnostic workflow head was `208b57328ad303bed601cc097f092e7e7816d99a`; artifact `source-commit.txt` pins the tested repository source to `88e24c56a3bbdc78270762a67232045f2c53f087`. Artifact12315389896 contains204files,3,468,054uploaded bytes; downloaded ZIP SHA256 is `9087cb7f671fb22d867b8d435f450828e5370201eb416db50ded3b9863f553b4`. The job explicitly required both `! -e` and `! -L` for `rust/tools/check_pandad_safety_padding.py` in the checkout. Its `retirement-XOmNld/executables.before.json` records `original_python_sha256: null`. Private admission roots retain other still-inventoried Python source pins as passive data; the removed checker is neither restored nor executed.

The absent/present native roots passed against the12-entry sealed `edb42bf0` original manifest (manifest SHA256 `3a12023217195181b9f1d05d06803a4b0add0438baf700abfeb5a7ad0f3f22ea`). `sanitized-gate.json` records present/absent exit0 and `original_live_exit_code: null`: this deleted-source run uses the pinned historical original observation, not a fresh original Python execution. The original and all four native report JSON values have the same canonical SHA256 `cd63f183124ec5d2615f1ed54998036e765545ef9a14e0aa9677b2d42e3e9639`; full stdout/stderr and request transcripts remain in the artifact. Each native lane has one CarParams plus four source and four native requests. The sanitized and plain original builds each bind21identical source hashes (canonical source-list SHA256 `85721a31b1dfe830f7f7bc7fe4c96cc73ec1c1ba0b5ff3bf93601c3328fd9d5f`) with different instrumentation. Both sanitized native roots completed untraced; each plain traced root recorded exactly four successful, expected exec vectors over the same three sealed executables, with checker SHA256 `e694531037e9ff4025f6165a3b794fb165917de268ffccfd587605546b73eb50` and native probe SHA256 `33e368f69e234454a7136127049606e5fadfc1450d5988cda683a7414865acac`.

The job reproduced the old finalization regression on `edb42bf0` as exit101 with the specific mutated-probe assertion, then passed the current absence test, four finalization tests, retirement test and strict Clippy. The separate sealed original negative-phase fixture contains two nine-artifact matrices (SHA256 `6a488184317862a18677d2a4406a88cc34351b7dfa441d2dd972a2c5abbfdcb7`); this post-delete job did not run `pandad_safety_padding_reference.rs`, so it is evidence retained by source, not an18-phase test result from this job. The original device and broader PR/ARM acceptance gates remain separate.
