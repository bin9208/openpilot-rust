# Fresh Params read cost (#286)

Tracked in [#286](https://github.com/bin9208/openpilot-rust/issues/286), based on
`a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`. This is a small-value read optimization;
it does not cache settings, change refresh cadence, or change stored bytes.

## Device update and baseline

The explicitly authorized C3X update used `git pull --ff-only rust dev` and the
normal launcher/verified release installer. Source and installed package advanced
from `9200ffe57` to `a3d0e3a49`. Persistent external recovery/web services needed
an additional orderly stop and normal launcher restart to replace their old,
deleted executable mappings; this distinct lifecycle defect is tracked in
[#287](https://github.com/bin9208/openpilot-rust/issues/287).

All 22 inspected runtime processes then mapped the new package. In normal
offroad driver preview, two separate 60-message model samples were entirely
valid, averaged 19.256/19.233 ms inference and 49.978/49.984 ms publication
intervals. Road, wide and driver camera samples were valid. No manager process
marked as required was absent. Device thermal status was green and Wi-Fi was
active. The monitoring policy's output remained invalid because offroad preview
lacks its required vehicle/model inputs, matching the original all-checks rule.
The test restored `DisableDM=2` and preview off. This is not driving acceptance.

The updated CarrotMan main thread performed 1,462 failing `statx` calls and
1,468 `newfstatat` calls during a three-second trace. An isolated one-byte Params
fixture reproduced `statx` returning `ENOSYS`, followed by `newfstatat`, for each
read. Instrumented syscall time is not process CPU time.

## Implementation and tradeoff

The reader opens the value normally and reads up to 256 bytes into a stack
buffer. It retries interrupted reads, continues short reads until EOF, and
preserves errors. A full buffer falls back to `File::read_to_end`, preserving
size-based allocation for large values. Missing files, exact boolean semantics,
binary/empty values, namespace behavior and atomic replacement remain unchanged.

The physical test uses the public Params API from `examples/read_cost.rs`, a
private temporary namespace and synthetic binary values. Old/new/old/new/old/new
runs use the same optimized GNU ARM toolchain and CPU 2; this affinity applies
only to the owned probe. Every read checks the complete returned bytes.

| Value size | Reads per run | Before mean us/read | After mean us/read | Change |
| --- | ---: | ---: | ---: | ---: |
| 1 byte | 20,000 | 43.279 | 37.186 | -14.08% |
| 64 bytes | 20,000 | 43.091 | 36.832 | -14.52% |
| 257 bytes | 10,000 | 43.918 | 47.928 | +9.13% |
| 65,536 bytes | 2,000 | 114.073 | 114.078 | +0.004% |

The extra initial read costs time just above the small-value boundary; this
tradeoff is explicit. The measured target is repeated small settings. These
numbers are isolated API timings, not whole-runtime CPU or thermal savings.
Ten one-byte reads issued 20 value-file metadata syscalls before and zero after.
Both probes exited zero. The original failing and corrected traces are retained.

## Validation and remaining gates

- All 12 package tests pass, including binary boundary sizes through 1 MiB,
  interrupted/short reads, partial-read errors, directory errors, atomic
  replacement, concurrent writers, namespaces and exact boolean semantics.
- All-target Clippy passes with warnings denied; formatting passes.
- Original C++ interoperability passes for all 367 registered keys, binary and
  empty values, remove/clear behavior and exclusive writer locks. The first local
  invocation lacked the pinned Cap'n Proto shared-library search path; retrying
  with that existing provider passed without installing or changing dependencies.
- Real-surface receipts are in `.omo/evidence/params-read-286/`. Raw device
  identity and captures remain private and are not committed.

The optimization is currently an isolated candidate. Review, required exact-SHA
CI, a source-matched runtime release and installation validation remain pending.
The installed runtime is still the verified `a3d0e3a49` release.

Docs-Not-Needed: internal Params I/O optimization with unchanged user-setting
semantics; no setting addition, removal or behavior change.
