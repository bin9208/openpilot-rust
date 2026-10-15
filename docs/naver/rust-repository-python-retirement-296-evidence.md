# Repository-wide Python retirement: current evidence (#296)

This is an unfinished conversion, not a completion report. Base commit:
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Work is isolated on
`codex/feat-296-repository-python-retirement`; concurrent `dev-md` work is not
part of this candidate. No device validation or release is claimed here.

## Inventory

The [source inventory](rust-repository-python-retirement-296-inventory.tsv)
records all 3,580 baseline Python/Cython/SCons source paths and their original
Git blob identities. It includes 3,542 `.py` files, eight Cython files and 30
SCons entrypoints. Twenty-two sources have locally verified replacements and migrated
callers; 3,558 entries remain pending. `replaced-local` does not mean merged,
published, or that its whole parent component is converted. Embedded and
extensionless Python execution still needs a separate complete caller audit.

## CI and synchronization

The native CI family preserves BaseLoader YAML scalar strings, policy guards,
required dependency gates and real shell bootstrap execution. Its 46 tests
passed. Independent negative mutations of each checker were rejected. The
three replaced Python policy/bootstrap test files are removed. The original
changed-files test remains because full Python syntax admission still requires
conversion; that retained test passes. The process-probe test was removed only
after its actual updater checker consumer migrated, as described below.

The native synchronization command preserves the eight original guard cases,
19 captured numeric-semantics cases, real CLI execution against fake `gh`,
post-update readback, summary append and failure ordering. Sixteen tests pass.
Tests remove GitHub tokens and never call the real service. The inherited
workflow remains restricted to `bin9208/openpilot` and is disabled in this
Rust repository; this conversion does not authorize dispatching it. Its three
workflow callers now use the native implementation/test. The two replaced
Python files are removed. A review caught toolchain installation before the
disk guard; explicit installation was removed so the existing guarded native
bootstrap owns that operation. After integration, all four workflow-policy
and 26 isolation tests passed against the changed workflows.

The updater command checker now executes a native child fixture and the actual
current Rust updater candidate. Its independent original baseline pins source
and fixture identities and explicitly marks frozen reference observations.
All three output/environment cases and two live/exited-leader SIGINT cases
passed; six native unit tests and Clippy passed. A deliberately faulty candidate
that reported interruption while leaking its worker was rejected and cleaned
up. Workflow callers migrated, and the old checker, source wrapper and Python
process-probe tests were removed. The original runtime process module remains
because other original-source comparisons still use it.

After these integrations and the additions below, all 117 selected dev-tools
tests passed together; all-target Clippy passed with warnings denied. The added
logging-build and native architecture-test disk guards are asserted separately
while preserving the exact arguments/hash of all 179 inherited ordinary guards
and all five raylib guards (186 markers in the Rust workflow).

## Panda source-oracle builders

The protocol, USB, SPI, peripheral and runtime-fixture builders now run natively.
Original captures cover command ordering, generated files, manifests,
sanitizer variants, failures and custom compiler selection. Eight native tests
pass. Nine real C++ original/native builds matched their commands and generated
inputs; the two shared USB fixture outputs were byte-identical. Independent
review found and corrected the missing runtime-fixture `--compiler` option.

Existing behavior comparisons covered protocol 3,174 cases/30,720 alert steps;
safety 889 scenarios/4,483 steps; device 1,746 scenarios/2,490 operations; state
16 scenarios/4,632 steps; USB 948 scenarios/3,886 operations plus concurrency;
SPI 122 scenarios/7,387 syscalls; peripheral 38 scenarios/30,240 steps; and
CAN I/O 2,167 scenarios. Eight malformed truncated CarParams cases have
nondeterministic behavior in the original C++ implementation; they are not
claimed as exact equivalence and Rust's malformed-input rejection is retained.
These builder comparisons do not replace the complete current-source Panda
CI gate.

The five Python builders are removed after migrating their orchestration
calls. The existing fresh-output/spidev orchestration regression now intercepts
the native command boundary and verifies its binary/source roots. All three
orchestration tests pass. JSON11 staging, the service-header generator and the
full original pandad source builder passed six native tests and strict Clippy.
All 88 service tuples and both original header newline modes match. Panda CI
now consumes the native staging result and builder, and the runtime-source
Python builder is removed. The shared Python logging helper was subsequently
removed after the remaining staging consumers migrated as described below.
The original Python service catalog still has other consumers and remains pending.

Real original/native runtime-source builds matched compiler arguments, logs,
6,564 header bytes and original manifest fields. The resulting linked
executables were byte-identical after removal of debug/build-ID sections:
SHA-256 `4f99e53d0be04c5eb4a3a4335bf665b9fa351f8a0dcd390f9ddbdc4b359e0245`.
Native receipts additionally identify the compiled Rust header generator,
catalog and emitted header, so original source fingerprints alone cannot
misrepresent generator provenance. Review regressions established explicit
errors for malformed JSON11 lock structures. This is host builder evidence,
not runtime/device acceptance.

## Logging and release utilities

The shared C++ logging-reference builder has a native implementation preserving
locked dependency staging, the fixture-clock-only override, ZMQ wrappers,
compiler command, failure artifacts and source provenance. Three native tests
and strict Clippy passed. Actual original/native executables were byte-identical
without stripping: SHA-256
`81684b60368a9d31ef5e66c70435af1bf282be33963d9c585319941ca5b530b5`.
The logging checker and its workflow now invoke this native builder. The logging
comparator itself is still Python.

The bootlog/loggerd builders preserve eight independently captured option sets,
compiler ordering, raw interleaved stdout/stderr, nonzero exit, empty failure
logs and timeout partial logs. Five native tests passed. Actual original/native
executables were byte-identical: bootlog
`204fa73615158ddfd81996be74b657c220418003fe0532aa3cd9c48dd13eec13`,
loggerd `2690c0b6f2d60dd042052ebea2e3ca54d7d5c4407307c38230ddddbfe268d001`,
and producer `99692b29868a33070904ebe32491ea9ee08877e57735d3be033f197b6da79f41`.
Their workflow callers migrated and both Python builders were removed.

The native logger-identifier checker compared 80 live original-C++/Rust cases:
two keys, ten counter inputs and four I/O failures. Its outputs/progress matched
the unchanged original Python checker, including 320 per-case JSON files.
Five native tests include a mismatched-prefix negative control. Nine original
path-resolution cases exposed and corrected symlink/absent-prefix handling in
a shared native resolver. The 80-case comparison passed again through an absent
prefix followed by `..`. A new 30-second execution bound is explicit. The checker
and workflow migrated and the original Python checker was removed.

The JPEG sanitizer checker preserves 15 original success/failure cases,
Cargo/compiler flags, exact ownership test selection, full codec sanitizers,
failure ordering and logs. Two native tests passed. Real original and native
ASan/UBSan codec/ownership runs passed using binary SHA-256
`431e0d18b794219d98bec164559154206a3494b7b1b2ebb37bfd9316223c4f3b`.
Summary and progress match; only parallel test completion order differs in the
raw run log. The workflow migrated and Python checker was removed.

The native message-bridge builder preserves six original captures, all eight
C++ sources, ZMQ discovery, generated service-header newlines, inherited raw
compiler output, timeout and failure artifacts. Three tests passed, including
a real CLI with no Python in its controlled PATH. Actual original/native
executables matched SHA-256
`3c8dd3a282a83e034ab91f7b7252e31928e18bbc44ceec5eb7b296bdf89ecb4e`;
the service header matched
`6086c8d503714be65cbcfe109c63e7fc763ec7bf81a365e53faf7d632f99c29e`.
The startup-services workflow migrated and Python builder was removed.

The remaining Params/encoder JSON11 staging consumers now invoke an explicit
native `stage-json11` binary. A composite action builds it for the host with
the disk guard before Cargo/toolchain work, clearing cross/sanitizer flags.
All 22 workflow uses and the carrot-man template pass the explicit binary.
Three native tests cover callers, action mutations, and real action shell
execution against controlled tools. Eleven existing Python orchestration tests
and Ruff pass. The shared Python logging/staging helper and architecture test
were removed; the architecture coverage now runs in `native_build_closure`.
The Params Cython builder and encoder orchestration remain Python and pending.
The real Params binding rebuilt with the native stager and explicit local SDK
headers; importing it and checking absent/Unicode STRING values passed. The
initial attempt omitted the local Cap'n Proto include prefix and failed to find
`generated-header-support.h`; supplying the existing `--capnp-prefix` resolved
that environment failure without source changes or a skipped check.
Native process support preserves ordered merged raw output and inherited
streams; ten tests cover errors, timeouts and process-group cleanup.

Native release-file selection matches the unchanged original regular expressions,
regular files, directory/dangling/loop symlinks and FIFO exclusion. A synthetic
original capture and a 15,640-path current-tree comparison matched. Three tests
cover selection, empty roots and a real shell bootstrap using controlled build
and disk-guard peers, including failure propagation, stdout separation, cross
environment removal and rejection of an in-tree build cache. The bootstrap
builds pinned Rust outside the source tree, preserving existing release copy
semantics. All three callers migrated and the Python selector is removed.
No destructive release script, Git push or vehicle action was executed.

## Model compiler: partial components only

Native launch validation preserves original dimension types and Unicode word
semantics. It matched 4,411 original cases using the genuine kernel binary
SHA-256 `fe72d39f60f599c761e5f40697380f1426c7d6ca963ae80f0232336175f12512`.
Native ONNX parsing matched 918 records from the real driver model SHA-256
`dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04`:
201 nodes, 209 initializers and 7,234,272 initializer bytes, including metadata.
Typed FP16/BF16 values retain the original numeric conversion behavior.

The driving model SHA-256
`f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5`
also matched 1,391 records: 352 nodes, 335 initializers and 60,006,787
initializer bytes. All 27 admitted generic operators lower; 1,630 independently
captured semantic/binding/image cases match, including typed arithmetic,
casts, reductions, original activation recipes and offline binding conversion.
The actual graphs lower to 617 driver and 1,016 driving plans. A separate
1,106-record original corpus checks intermediate dtype/shape admission.

The compiler-specific memory planner preserves the original 256/32 TLSF
configuration. Its 109 generic cases and two captured full-model schedules
match offsets/arenas: 20,293,376 bytes for driver and 69,575,168 for driving.
These are captured schedules; this does not establish native fusion/scheduling.
The bounded f32 renderer matches 42 host-compiled original cases. The genuine
A630 offline QCOM compiler under QEMU produces byte-identical buffer-add kernel
bytes. This does not validate GPU execution or the full image renderer.

An initial CPU backend compiles a supported f32 elementwise/view subset into
real ELF objects/libraries, validates symbols and architecture, and publishes
through the existing atomic CPU artifact emitter. Six integration tests cover
original elementwise/broadcast and offline binding outputs, a three-node graph
with memory planning on/off, and admission/compiler failures. Unsupported
operators fail closed, without a Python fallback. The complete model-runtime
package passed 107 tests and all-target Clippy before the subsequent independent
review repair. Review identified unpinned external initializer data at this new
backend boundary. The backend now rejects external-data models before payload
I/O or compilation; the generic importer retains external-data support. A new
regression first reproduced publication with the old boundary, then verified
rejection with unchanged protobuf SHA and two different weight payloads, plus
a missing payload, without publishing artifacts. All six backend tests, the
generic external-data import test and all-target Clippy pass after this repair.

Full compiled-model numeric parity remains unresolved. After preserving IMAGE
half-storage/float32 accumulation semantics, the driver comparison still has
553/553 different outputs, maximum absolute difference 0.004296779632568359.
No tolerance was loosened or failure accepted. Original LLVM reference output,
model/source hashes, flags and exact comparison bytes are retained in the
compiler fixtures; the original CLANG run failed with an undefined `fmaxf`
ELF-JIT symbol and is not passing evidence. Optimized fusion/reduction order,
complete typed code generation, CPU/QCOM/AMD export, actual full-model outputs
and tinygrad training/development functionality remain pending. The portable
original corpora are consumed without Python. No model Python source was removed.

## Outstanding delivery evidence

Checkpoint `30866c90dc246fddf2c963b3e5f0947390bee30d` was pushed to the
isolated issue branch. Its [Fast checks run](https://github.com/bin9208/openpilot-rust/actions/runs/38716577954)
failed in the logging-builder fixture: it froze the capture machine's unsorted
filesystem order for schema/generated files. The same failure was reproduced
locally on tmpfs. The repaired test uses the current directory enumeration order
while requiring the exact original file multiset, argument slots, fixed source
order and all other command/artifact fields. All five logging tests pass and the
eight-case original comparison also passes on tmpfs. Production enumeration is
unchanged; no source/flag checks were removed. A new exact-SHA cloud run is needed.

Continue the [implementation plan](rust-repository-python-retirement-296-plan.md)
through every remaining source family, actual consumer migration, a zero-Python
execution audit, independent review, exact-commit PR and merged-dev checks,
source-bound packaging and applicable device gates. Current local results are
not Actions evidence and do not establish repository-wide completion or CPU
savings. Issues #296 and #1 remain open.

Docs-Not-Needed: internal implementation and validation records; no setting or
user-guide behavior change.
