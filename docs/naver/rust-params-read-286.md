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
Both probes exited zero. The original failing and corrected traces remain in
private local evidence; they are not part of a fresh Git clone.

## Matched CarrotMan process comparison

A subsequent C3X A/B/A comparison built both versions with the same GNU ARM
package-only release configuration. Affinity stayed on cores 0-3, with
SCHED_OTHER and nice 0. Each phase warmed up for 10 seconds and then measured
about 60 seconds of process CPU time.

| Phase | CPU, percent of one core |
| --- | ---: |
| Original `a3d0e3a49` | 8.051937 |
| Candidate `87cc8dc1c` | 7.369416 |
| Original `a3d0e3a49` | 8.218945 |

The two original phases averaged 8.135441%, giving a reduction of 0.766024
percentage points, or 9.4159% relative, for this offroad process comparison.
All 180 compared `carrotMan` payloads were identical and valid; all 180
`navInstructionCarrot` payloads were identical and invalid without a route.
Publication cadence remained approximately 50 ms. The temporary child was
stopped, both managers were resumed, and the installed original process was
confirmed running afterwards. This does not establish total-device CPU,
thermal stability, or driving performance.

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

An independent review approved the change and reproduced the focused checks.
All 65 PR checks passed for `87cc8dc1c366b2eb1015fa88acb93ae218421cbc`
([Actions](https://github.com/bin9208/openpilot-rust/actions/runs/38497987784)).
[PR #288](https://github.com/bin9208/openpilot-rust/pull/288) merged into dev as
`f3446f65e1cbd894aba1aac7e724c12e6d89ff61`; the complete source tree is identical
to the reviewed PR tree.

The exact merged-SHA local AGNOS pipeline passed all 13 stages: 87 binary
builds, host startup support, staging, model staging, ABI audit, model admission,
inventory, normal bootstrap, manager/log-upload integration, sealing, static
installer provenance, and packaging. A fresh premerge-tree installation using
the real ARM installer and launcher under QEMU verified 1,071 file hashes and
162 ARM ELF headers. This is separate from physical installation acceptance.

The first local build attempt used a deleted libyuv cache path. Reusing the
existing pinned ARM provider, verified by its recorded hash, fixed the local
environment without a source change.

On 2026-10-14 the user took the C3X away and requested a Git handoff. No device
test or installation remains running. At the last connected verification the
installed runtime was still `a3d0e3a49`, with `DisableDM=2`, preview off and both
managers running. A final read attempt after the departure request reported
the device absent; current device state was not re-verified.

The separate [merged-SHA Rust run](https://github.com/bin9208/openpilot-rust/actions/runs/38505272602)
was still running at handoff. Publication and physical installation of the
optimization remain pending. Continue from the
[cross-computer handoff](rust-c3x-optimization-handoff-286.md); do not repeat the
completed profiling or describe the optimized runtime as already installed.

Docs-Not-Needed: internal Params I/O optimization with unchanged user-setting
semantics; no setting addition, removal or behavior change.
