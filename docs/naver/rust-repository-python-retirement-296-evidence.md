# Repository-wide Python retirement: current evidence (#296)

This is an unfinished conversion, not a completion report. Base commit:
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Work is isolated on
`codex/feat-296-repository-python-retirement`; concurrent `dev-md` work is not
part of this candidate. No device validation or release is claimed here.

## Inventory

The [source inventory](rust-repository-python-retirement-296-inventory.tsv)
records all 3,580 baseline Python/Cython/SCons source paths and their original
Git blob identities. It includes 3,542 `.py` files, eight Cython files and 30
SCons entrypoints. Ninety-two sources have locally verified replacements and migrated
callers; three are partially replaced and3,485 remain pending. `replaced-local` does not mean merged,
published, or that its whole parent component is converted. Embedded and
extensionless Python execution still needs a separate complete caller audit.

The first execution-edge audit scanned 12,700 tracked entries and 11,965 text
files, recording seven symlink aliases without following them. The
[reviewed edge inventory](rust-repository-python-retirement-296-execution-edges.tsv)
records 27 concrete findings with source blob identities. It identified 48
literal embedded command sites: 29 in the Rust workflow, one in the carrot-man
template, one in the route-vault workflow and 17 in other shell/Jenkins/vendor
files. Six Python source generators emit extensionless Python fixture commands.
Dynamic interpreter variables and embedded C interpreter calls remain pending.
No additional tracked Python shebang entrypoint outside the baseline was found;
this scan is not a completeness proof. The selected native launcher remains
Python-free; retained legacy launch paths are accounted for separately.

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

After these integrations and the additions below, all 136 selected dev-tools
tests passed together; all-target Clippy passed with warnings denied. The added
logging-build, encoder-build, model-source and native architecture-test disk guards are asserted separately
while preserving the exact arguments/hash of all 179 inherited ordinary guards
and all five raylib guards (188 markers in the Rust workflow). The original model
LFS fetch now precedes workspace tests because the new compiler corpus requires
actual model bytes; it does not accept an LFS pointer or skip that validation.

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

The firmware SPI fixture build driver is also native: 26 original cases and two
native tests preserve compiler commands, separately captured text/decoding,
failure ordering, lexical paths, CLI semantics and late provenance. Python ABI
metadata is supplied explicitly by the remaining checker. Fresh original/native
libraries match SHA-256 `6c79a5eb54544d5d7ed21fb5e6e9513b82e03ff8fc038ec46c1948f89012fdd4`
(fixture) and `6e5e62aa775f2c5366bd05b1577f99677585ab0a9c6b1bcd40bff64335e8cf90`
(binding). Three owned `/dev/null`-backed comparisons match 40 calls with both
builds; no physical SPI was accessed. Original C-extension cached handles are
recorded separately; the native Rust zero-handle requirement is unchanged.
The caller migrated and the Python driver was removed. The C extension and
Python comparison remain pending. The orchestration now fingerprints all native
Panda build tools and their source dependencies; three orchestration tests pass.

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
streams. File-backed compiler logging additionally preserves raw ordered output
above the old 16 MiB memory-capture cap without buffering it in memory. Thirteen
process tests cover large logs, errors, timeout partial files and owned process
group cleanup. Independent review found no remaining issue in that file sink.

The encoder source builder preserves all 17 C++ sources, generated schema/header
inputs, explicit dependencies, output collisions, disk failures, raw compiler
logs and the original 180-second timeout. Twenty-seven original cases additionally
cover duplicate option last-value behavior, equals syntax, unique abbreviations,
help ordering, Unicode decimal-number admission and parse exit code 2. Six native
tests and strict Clippy pass. Review corrected superscript/circled-number
admission and the original rejection of a trailing option terminator when no
positional arguments are defined; observed original failures remain failures.
Fresh original/native executables match SHA-256
`126ed49990c66b0e93746578d5ec73f6ef582f3216e43641518cddc97fa1658a`.
The orchestration explicitly selects the native builder and fingerprints its
source/executable; ten orchestration tests pass, including real command-boundary
success/failure without Python in PATH. The workflow builds the current builder,
and the replaced Python source was removed.

The embedded UI updater handshake checker is now native. Two tests preserve 32
independent original receipt/trace cases, Python JSON admission, exact strace
arguments/deadline, raw inherited output and failure ordering. Both the original
heredoc and native CLI passed real strace fault injection against the same freshly
built UI test executable, SHA-256
`3cd04b4dd936c50e93791c050b27283a74b34e0d2554519f080dc9c6ef82d50d`.
Each trace has one delayed EAGAIN, then same-worker CLD_EXITED waitid and accepted
queued connection. The original lifecycle assertion remains intact; the workflow
now invokes the built native checker. This retires one embedded execution edge,
not another source-file inventory entry. Independent review found no Important
issue in this conversion.

The Athena VisionIPC source builder now preserves all ten C++ sources, CXX as
one executable, inherited output, lexical path behavior, overwrite/failure
artifacts, original CLI forms/exit codes and the exact 26 GiB disk admission.
Thirty independently executed original cases and two native tests pass. A fresh
original/native build matches the entire executable SHA-256
`ad6297d65986c9935923e6478691a8daf0c415646cd5da83e8350c6985887a25`,
including stdout/stderr bytes. The real Python orchestrator was exercised with
controlled subprocess boundaries: all 20 scenario names/order remain and only
the builder selects the native binary. The original builder was removed; the
remaining Athena comparisons still use Python. No peer/device/namespace was run.

The portable, dependency-free `openpilot-service-tools` package replaces the ADB
wrapper's embedded service-port generator. Exact 88-row output, eight FNV
vectors and all eight original shell command/exit cases match without Python in
PATH. The original process-substitution behavior still permits SSH after a
producer failure. Sixteen native tests and strict Clippy pass, including Linux
and macOS host selection, warm/offline toolchain reuse, pre-build disk guards,
output separation, and combined missing-parent/symlink cache rejection before
source mutation. A real Linux bootstrap built the current package and emitted
88 byte-identical rows. macOS controls were simulated; real macOS execution is
unverified. No actual ADB, SSH or device operation was performed. The shared
Python service catalog remains because other callers still require it.

The hardware command assertion is now a native checker. Thirty independent
original JSON/stderr cases preserve duplicate-key behavior, numeric equality
(including true/one and integer-valued floats), invalid data and exact stderr.
Three tests and strict Clippy pass. The actual owned command fixture was freshly
built and both checkers passed its identical outputs, fixture SHA-256
`a3fd7bb26e8703f55f7426ac08babe9f6d6713ea2d37bd7e46cab82930f1ca26`.
The workflow retains the original fixture execution and redirections, replacing
only the inline Python assertion. No hardware platform/I2C/device operation ran.

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
The bounded generic renderer matches 44 host-compiled original cases. The genuine
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

The next typed preprocessing unit preserves CPU:LLVM's emulated half arithmetic
separately from native OpenCL half operations. Thirteen independent CPU:LLVM
cases include all 65,536 half bit patterns and actual dynamic inputs; the first
nine driver-model preprocessing output hashes match. Review exposed two real
differences: f32 reciprocal multiplication changed 7/3 and overflowed a finite
tiny-input quotient, and numeric half load/store changed raw Identity/view bits.
CPU division now follows original FDIV rewriting for both admitted float types;
pure half storage views copy raw words, while numeric conversions retain their
conversion semantics. Independent review confirmed both fixes. Eight backend
and six renderer tests plus strict all-target Clippy pass. The current renderer
again produced the exact genuine 2,144-byte A630 Add binary. This validates the
bounded preprocessing unit, not the full convolution/image/fusion compiler.

The subsequent bounded CPU reduction/image-convolution unit passed all 115
model-runtime tests and strict all-target Clippy. Twenty-eight original mean
reductions and 14 small/pinned-weight convolution cases match exact bytes. The
actual first driver convolution also matches all 2,764,800 output bytes, SHA-256
`7110aa5f009b930ed61715b500c6f9c127803db7c8c1adc50a7e9a774c025e7b`.
Explicit storage round trips, guarded padded loads, original accumulation order
and explicit fused multiply-add preserve the inspected CPU:LLVM behavior.
Generic/IMAGE MatMul products, nested/oversized reductions and GPU reduction
scheduling remain rejected. Independent review found no concrete Important
issue in this bounded unit; it does not establish arbitrary NaN-payload,
cross-target or full-model equivalence. The genuine current A630 Add binary
remains identical, with both renderer modules fingerprinted.

The CPU math unit subsequently passed all 120 model-runtime tests and strict
Clippy. Exp2, Sign, contracted multiply-add, tanh-Gelu and Sigmoid reproduce
14,752 independently captured dynamic outputs bit-for-bit. The actual first
driver Gelu also matches all 2,764,800 bytes, SHA-256
`3c3726b13ef0cf54ccf8b7be3cf6b618d70c5665780f11a6edeac82c2eaf5519`.
Original polynomial/constants, range handling and explicit FMA order are retained.
Independent review found no concrete Important issue in bounded conversions,
shifts, target separation or admission. Gelu-none/default remains rejected before
compilation/publication: its unresolved NaN payloads depend on original layout and
output materialization context. Twelve independent cases and both original
capture contexts remain recorded as pending; no universal NaN normalization or
tolerance was substituted. Genuine A630 Add output remains unchanged with the
new math helper source fingerprinted. Full-model fusion and parity remain open.

Full compiled-model numeric parity remains unresolved. The last complete scalar
reference comparison after preserving IMAGE half-storage/float32 accumulation has
553/553 different outputs, maximum absolute difference 0.004296779632568359.
No tolerance was loosened or failure accepted. Original LLVM reference output,
model/source hashes, flags and exact comparison bytes are retained in the
compiler fixtures; the original CLANG run failed with an undefined `fmaxf`
ELF-JIT symbol and is not passing evidence. Optimized fusion/reduction order,
complete typed code generation, CPU/QCOM/AMD export, actual full-model outputs
and tinygrad training/development functionality remain pending. The portable
original corpora are consumed without Python. No model Python source was removed.

The native msgq sanitizer checker replaces its Python driver and its CI call.
Eighteen independent original cases preserve four-artifact emission order/count,
malformed JSON rejection, strict captured UTF-8/universal newlines, inherited raw
stderr/stdin, ASan lookup and early failure. Three native tests include actual
controlled CLI stream/ordering comparisons. The original and native checker both
passed the four actual ASan/UBSan/leak-detection test executables; all four binary
hashes remained identical and native peers retain sanitizer symbols. Concurrent
nested test output interleaves, so raw logs/line counts are not byte-identical;
all test-name occurrence counts match. Four-GiB disk admission, two Cargo jobs,
disabled incremental compilation, bounded preparation and owned process cleanup
are explicit execution-policy additions; the original per-test 60-second timeout
and assertions remain. No device operations were performed.

The five Panda builders now share a tested opt-in argparse scanner. Independent
original capture exposed accepted-option and failure-effect regressions, including
creating an output directory for a missing value. All 123 original CLI cases and
eight existing builder tests pass after repair. Exact/abbreviated/attached flags,
ambiguity before help, repeated identical mode flags, Unicode negative values,
space-containing values and exit status 2 are preserved. No actual build semantics
were changed. Other command families require their own migration evidence.

The shared native process runner now supports concurrently fed piped input,
separately inherited stderr and explicitly inherited stdin. Twenty focused tests
cover raw bytes, backpressure on both output pipes, early input closure, empty
input EOF, incompatible modes and timeout/descendant cleanup. Existing defaults
are unchanged. Independent review found no concrete Important issue.

The control-tools policy comparator is now native and both CI consumers migrate.
Fifty-five independent original cases preserve numeric/NaN equality, UTF-8/16/32
input, raw-versus-escaped surrogate identity, distinct object keys, deep JSON
admission and receipt-serialization boundaries, exact partial artifacts and
failure ordering. Actual original joystick/input producers generated 11 cases /
4,216 steps: both native probes pass with byte-identical original/native output
and receipts. Two deliberately corrupted expectation sets reject with identical
mismatch receipts. Independent review found and verified fixes for surrogate
pair conversion, receipt key collapse and empty-container depth limits. Native
comparison uses iterative traversal and a documented 300-second owned-process
deadline. Source producers, Python bindings and their compilers remain pending.

The encoder, Athena, firmware SPI and msgq command interfaces also use the
shared scanner after 82 independent original parser captures. Existing behavior
fixtures still pass, as do 34 real native help/error invocations. This includes
ambiguity before help and Athena's required positional destination, without
changing build/check logic. The focused 83-test set and strict Clippy pass.

The CPU model compiler's IMAGE storage-alias unit passed all 124 package tests
and strict all-target Clippy. Thirty-seven independent positive graphs and eight
rejection graphs preserve computed-input half storage, runtime-input handling,
padded channels and retained full-precision graph outputs. Actual final-output-
only driver prefixes 30 and 59 match original bytes; forcing every intermediate
to materialize is explicitly a different diagnostic context. Independent review
found unsupported view-ancestor ALU consumers and dynamic weight aliasing;
regressions reproduce the original behavior and now reject unsupported graphs
before artifact publication. Effective auto_pad overrides are preserved. These
are bounded CPU scheduling results, not full-model or GPU equivalence.

The current combined local tool routing step passes 211 dev-tools tests
(including all 58 library tests) and 16 service-tools tests. Two additional
log-format codepoint tests pass, strict all-target Clippy passes for both changed
tool/JSON packages, and workspace formatting/diff checks pass. The model package
separately passes all 124 tests and strict Clippy. This checkpoint is local
validation; its exact remote CI remains required.

The native registration-vendor verifier replaces the Python archive checker and
its CI invocation. Thirty-seven independently captured archive/filesystem cases
and five CLI probes pass. Raw/gzip/bzip2/xz inputs, member ordering/duplicates,
path normalization, non-UTF-8 filenames, local symlink behavior, changed-file
policy and error artifacts are preserved without extracting archive members.
The exact published charset-norm 3.5.1 archive verifies 28 files; original/native
reports are byte-identical, SHA-256
`7f3513df8861b8f46d317ee6a6c9efb1c4fd717d4654e6ce072d59db822b70c7`.
Both reject a mutated archive with no output file. The narrow empty GNU-sparse
header compatibility preserves original acceptance only after archive SHA and
raw header checksum checks. Independent review found no Important issue; strict
Clippy passes. bzip2 reuses the already locked/cached 0.6.1 package.

Five further builder/checker CLIs migrate through 73 independent original parser
captures: bootlog, loggerd, Panda runtime, bridge and JPEG. Their 83 focused tests,
25 real safe CLI probes and strict Clippy pass. A separate actual bridge missing-
dependency regression now preserves original exit 2 and creates neither output
nor target cache; all four bridge tests pass. Build/check implementation remains
unchanged by these parser repairs.

The two UI model/training adapter sanitizer drivers now have native commands.
Twenty-six independent original cases preserve newest-artifact selection,
filesystem-order ties, commands, raw logs, missing artifacts and partial failures.
Twelve additional original cases preserve arbitrary Unix filename bytes in the
root, target, header, library, raylib and evidence paths. Thirteen original parser
cases and actual native raw-argv/CLI tests cover the lossless optional process
interface; old UTF-8 callers retain their existing behavior. Native compilation
checks disk before each compiler, with explicit 1,200-second build and 300-second
probe bounds added to the original unbounded orchestration.

Both original and native UI adapters passed on private owned Xvfb displays.
The runtime archive, both ASan/UBSan executables, PNG captures, all three command
logs and driver streams match byte-for-byte. Complete receipts and commands match
after normalizing only their evidence directories. Model capture: 5,626 bytes;
training capture: 208,080 bytes. The UI CI suite now calls both native validators
on its owned display, records their source/binary fingerprints and retains every
existing source comparison. The two Python drivers are removed. Independent
review found no Important issue in the native commands or byte-path support.

The subsequent CPU IMAGE MatMul/Gemm unit initially passed all 127 model-runtime tests and
strict all-target Clippy. All 115 independent original shape/numeric cases and
actual driver prefix 97 match exact bytes; five unproved alias/weight/scaling
cases reject before publication. Generic non-IMAGE products and nonunit Gemm
alpha/beta remain unimplemented. Independent review found no Important issue in
this bounded unit. A final-output-only survey of driver prefixes 60–96 found
one remaining fusion mismatch at prefix 93: 3/960 float32 values differ by at
most 7.450580596923828e-9. Its materialized-input mean matches the original, so
this is retained as an optimized fusion/layout issue, without relaxing tolerance.
Later half-storage rounding hides those differences in prefixes 94–97; those
successes do not complete prefix-93 or full-model equivalence. Full current CPU
compilation still rejects an unsupported shared matrix operand/view alias.

Further original graph checks exposed reverse view aliasing: an arithmetic
consumer could read Identity/Reshape views of a packed operand without the
required storage treatment. Four initial original/native probes reproduced
wrong accepted outputs; same-dtype Cast siblings were then included. The repaired
identity graph tracks both ancestors and descendants, rejects unproved arithmetic
reuse before publication, and leaves held raw outputs unchanged. Six negative
view cases and two positive held-Cast cases are retained. All 129 model-runtime
tests and strict all-target Clippy now pass, with independent scoped review
finding no remaining Important issue. This fix was completed before publishing
the local alias/compiler checkpoint.

A subsequent cross-check found six original-successful raw-path invocations
that still failed in the registration-vendor and control-policy CLIs. Both now
use lossless positional/option parsing; control executes the canonical binary
path as native OS bytes and writes the same surrogateescaped command receipt.
All six actual original/native comparisons now exit successfully with identical
receipts, streams and artifact sets, including an ASCII symlink resolving to a
non-UTF-8 binary. Forty focused tests and strict Clippy pass; independent review
found no Important issue. Broader byte-path migration for older native tools
remains part of the outstanding execution audit.

The latest actual fast-checks routing step passes 232 dev-tools tests and 16
service-tools tests, plus its remaining Python policy test. All-target dev-tools
Clippy passes. The UI orchestration's six tests and Ruff checks also pass. These
local results are scoped to the current tools; model checkpoint admission review
and exact-commit Actions remain separate.

Further original final-export checks found that CPU:LLVM keeps some logically
half-typed arithmetic in float32 until a physical storage boundary. Native
materialization introduced rounding absent from the original. Five independently
captured float32-export graphs now reject before publication in both binding
modes; four physical-F16, exact-U8 and IMAGE positives remain accepted. The old
first-nine forced-node hashes remain diagnostic data, but are no longer treated
as final-export equivalence. All 131 model-runtime tests and strict all-target
Clippy pass. Independent admission review found no Important issue. Proper
virtual-half storage profiles, the prefix-93 fusion difference and full-model
compilation remain pending; no compiler Python has been removed. Fresh current
A630 Add source and its 2,144-byte genuine binary still match the original.

An additional 18-case original UI artifact-glob capture exposed six native
ENOTDIR failures when an intermediate path is a regular file or file symlink.
The native candidate lookup now skips NotADirectory like the original glob.
Dangling final headers and dependency candidates still fail on their original
stat operation. All 18 actual CLI outcomes match, the five focused tests and
strict Clippy pass, and independent review found no Important issue. The final
routing run includes this repair; workspace formatting, diff checks and all six
UI orchestration tests pass.

The CPU logical-half limitation above is now repaired for the independently
captured Cast, static-view and basic arithmetic families. A separate physical
storage projection keeps unheld logical-half intermediates in float32, while
preserving real F16 inputs, constants, selected outputs and their view ancestors.
Scheduling, allocation sizes, kernel loads/stores and wrapper pointer types use
the same projection. Thirteen held/unheld/view cases and fresh joint-first-nine
float32 exports match original bytes. True-final calibration and image prefixes
0/1/2/8/9 are exact. The known prefix-93 fusion mismatch remains unchanged.

An independently reproduced scalar-infinity division difference is also fixed:
the original CPU compiler folds the captured division contexts to positive zero,
including special numerators. Eight same-context ONNX cases cover both bindings,
f32/physical F16 and both memory modes; literal cases retain their own evidence.
Unproved shared/standalone infinity reciprocals still reject. All 134 model tests
pass on the final source, strict Clippy passes, independent review found no
Important issue, and the refreshed genuine A630 Add binary remains byte-identical.
No model/compiler/training Python has been removed.

The Panda and identifier CLI audit found seven raw-path failures and six logger
argument-admission differences. Native process arguments, environment overlays
and filesystem JSON now retain Unix bytes, while ordered runner options and
syntax errors follow the original parser before any child or output is created.
The negative-number classifier uses the pinned Python 3.12 Unicode 15 decimal
set: 680 digits in 64 ranges. Newer Unicode digits no longer bypass original
argument errors. Twenty-nine original Panda cases preserve modes, raw paths,
commands and partial failure products. Another 42 captures preserve strict
UTF-8 decoding and universal newlines across all seven compiler stages: invalid
bytes leave the already-created compiler product but no new command/success
receipt. The 110 focused tests and strict Clippy pass; actual original/native
raw-path, argument, Unicode and invalid-output comparisons also agree.

The native startup safety/resource sanitizer drivers replace two additional
Python sources, bringing the local retirement count to 32 of 3,580 baseline
entries (3,548 remain pending). Seventy-four original cases preserve compiler
commands, raw invocation-prefixed logs, 180-second deadlines, failure artifacts,
disk checks and filesystem behavior. The common Path.glob implementation now
scans every literal component, skipping inaccessible branches like Python while
retaining final dangling candidates for the original strict stat failure. Six
additional original model/training cases exercise this shared permission fix.
Safety path resolution preserves output creation before later library/target
resolution failures. Independent review found no Important issue.

Both complete original and native drivers passed on owned private Xvfb displays.
The bridge/Params ASan executables, CXX runtime archive, PNGs, full result reports
and driver streams match byte-for-byte. All command/program logs match after
replacing only each family's output directory. Resource disk receipts keep their
actual measured free-byte values and the same 1-GiB growth estimate; free bytes
are intentionally not presented as equal across separate runs. The native safety
CLI adds a 25-GiB reserve plus 1 GiB before each compiler, retaining its original
strict initial 25.25-GiB check. The remaining shared startup UI orchestrator and
its original adapter function stay pending because other callers still use them.

Final local validation for this checkpoint passes 247 selected dev-tools tests
(the routing suite plus the newly routed glob test), 16 service-tools tests and
all 134 model-runtime tests. All-target dev-tools/model Clippy, workspace
formatting, six remaining UI orchestration tests, Ruff and diff checks pass.
These are exact local scope results; the newly published head needs its own
Actions results and does not inherit earlier checkpoint success.

Two fixed-input UI comparison drivers now use complete independent original
corpora: 64 QR cases (208,400 modules) and 1,603 torque arc steps (135,829 points).
Compressed input/expected bytes carry integrity hashes and source/dependency pins;
expected answers never come from the native UI implementation. Thirty original
failure/artifact cases, eighteen CLI cases, strict text/JSON admission and the
independently measured torque serializer depth preserve the original checks.
Original and native checkers run against freshly built current-source examples
with identical five output files and stdout; raw Unix path comparisons also pass.
Independent review found no Important issue. The two Python driver files are
removed and all fifteen UI-suite commands retain their original ordering.

These are frozen Linux x86_64 original baselines, not dynamic original-source
recomputation or ARM equivalence evidence. Relevant source/dependency drift fails
closed and requires an independent recapture. The torque source helper and runtime
module remain tracked because other comparisons, including Miri, still use them;
the native source-pin reads are also explicit remaining dependencies. The current
source inventory has 34 retired files and 3,546 pending entries.

The bounded native CPU conditional unit now supports verified logical-f32 Concat
and Relu with lazy branches and checked per-branch index bounds. The source
Concat padding boundary remains distinct from direct arithmetic alias reuse.
Twenty independent Concat graphs cover axes, shapes, held outputs, signed zero
and NaN payloads; original barrier/Relu cases and negative aliases remain explicit.
Actual true-final prefixes 99/100/101/105/112 match. Mixed-half prefix 98 and the
full model still reject before publication; prefix 93's earlier fusion mismatch
remains unresolved. All 140 model tests pass on the final source, strict Clippy
passes and fresh A630 Add source/binary still match. No model Python is removed.

Native UI CI infrastructure is also implemented and independently tested, but
has not replaced the remaining Python orchestrator. It preserves peer selection,
raw filesystem paths, a separate inherited Xvfb display descriptor, command
receipts and cleanup. The safe `command-fds` 0.3.3 dependency is pinned; parent
descriptors remain CLOEXEC and no workspace unsafe code was added. Nineteen
original peer cases, thirteen display cases and six collector cases are covered
by twelve tests, including inherited stdin and unrelated-exec descriptor closure.
A private actual Xvfb/XTEST run passed and its owned process was reaped. Explicit
new policies bound the complete display line (15 seconds/64 KiB) and collected
streams (64 MiB plus a caller deadline); the original partial-line/collector hangs
are not reproduced silently. Import/version/cereal/XTEST/fingerprint gates and
ten remaining Python comparisons are still pending. This module retires no file.

Final local checks for the latest unit pass 264 selected dev-tools tests, 16
service-tools tests and all 140 model-runtime tests. The separately invoked
private-Xvfb integration test also passes; it remains opt-in in the ordinary
suite. All-target dev-tools/model Clippy, workspace formatting, six UI caller
tests, Ruff and diff checks pass. After deleting the two source drivers, both
native QR/torque checkers also pass with `PATH=/nonexistent` against the actual
native examples. This proves those command boundaries do not execute Python,
not that the remaining repository is Python-free.

## Native State, Scheduler and dynamic translation gates

The State and Scheduler checker drivers are replaced by `check-ui-state` and
`check-ui-scheduler`. Independent original corpora preserve all 2,400 UI state
frames and 160 display sweeps, including exact serialized CarParams bytes,
source/schema/dependency pins, original flattening order, Python numeric equality
and the unchanged absolute float bound of 1e-11. Forty-seven candidate failure
cases and eighteen CLI cases pass. Fresh actual native examples produce identical
25 artifacts and stdout through both unchanged original and Rust checkers;
twelve real subprocess failure/newline comparisons also match. Raw candidate and
output paths and inherited raw environment values are preserved. The original
State source's raw-root Unicode schema-loader failure is recorded as a removed
interpreter limitation, not claimed as successful source/native raw-root parity.

These two fixed-input gates use frozen independent Linux x86_64 expectations,
not dynamic original recomputation. Source or locked dependency drift fails
closed until independent recapture. Generic `state_source.py`, UI state,
scheduler and dependent original runtime helpers remain pending. Only the two
fixed checker drivers are deleted. All fifteen UI-suite commands keep their
order and disk guards; eight Python and seven native commands now remain.
The source fingerprint includes the native tools' embedded fixture data.

`check-ui-translations` instead parses current PO files dynamically with an
independent native reference implementation; it does not call the candidate
translation parser to construct expectations. Forty-seven original PO mutations,
twelve catalogs, 5,733 plural-selector outputs and 58 complete original CLI cases
cover parser semantics, language initialization, Unicode decimal/whitespace and
filesystem ordering, output boundaries, errors and Python JSON equality.
The actual original and native checkers produce identical source/candidate JSON
files, stdout and stderr across all twelve current catalogs. Source receipt SHA
is `1f3d1dec24f561a434ca6ff276db908c28bc7b19cabce1e2dd7286710a5eab3f`;
candidate receipt SHA is
`9ed4490f2e0b75ec63389ce6328405b56cfcd7a925e7081e7034cbfe33298aaa`.
The source semantics are pinned; catalog edits remain supported. An explicit
new per-entry allocation policy caps plural expansion at 1,048,576 forms with
checked allocation; it is not a global memory cap. The original translation
runtime and other translation tooling remain pending.

The CI translation call uses the native binary and an explicit single-tool build
with a 512 MiB growth guard. The original 179 ordinary workflow disk guards and
their hash remain unchanged; the new guard has a separately tested category.
All three native commands pass against actual examples with `PATH=/nonexistent`.
The inventory now records 37 replaced sources and 3,543 pending sources; neither
these gates nor their successful comparison establish repository-wide retirement.

## Mixed CPU conditionals and the observed fusion boundary

The native CPU compiler now admits independently verified mixed logical-half/f32
Concat inputs with f32 outputs. Ninety-six original graphs pass in both memory
planning modes; four unproved inverse-view half matrix aliases remain rejected.
The untouched pinned driver model `dee5a294...` compiles and executes through 406
native calls with all 553 f32 outputs (2,212 bytes) matching the original for one
recorded host input. Output SHA-256 is
`28468af72225b086839c4ae5de6fa07a1d01e843d02fd915d67f810e438549d3`.
This is one whole-model input proof, not all-input, driving-model or GPU parity.

The earlier true-final Conv/Gelu/ReduceMean export still differs at three of 960
f32 positions. Its original fused schedule and the separately materialized
comparison are retained as evidence. A structural storage-aware ancestry guard
rejects observable unproved f32 fusion before publication, including intervening
identity views and Add-zero/Mul-one expressions, downstream arithmetic and
multiple outputs. It does not match model IDs or node names. Twenty-three
independent original contexts cover rejection and materialization controls;
held-view and quantization boundaries retain their scoped positive evidence.
The guard does not implement the missing fused schedule or prove that half
quantization masks every input's differences. No tolerance was relaxed and no
model Python was removed. Final model source passes 146 tests, strict all-target
Clippy and independent review. Fresh A630 Add source and 2,144-byte compiled
binary remain identical to the original proof. Full compiler, GPU, training and
development functionality remain open.

Final local validation for this three-checker unit passes 274 selected dev-tools
tests and 16 service-tools tests in the actual fast-checks routing. All-target
dev-tools/model Clippy, workspace formatting, six UI caller tests, Ruff and diff
checks pass. Clippy initially identified placement of the new Unicode digit test
module before production items; moving that unchanged test module to the end
resolved the warning without altering behavior or suppressing the lint. The
separate reviewed model source passes all 146 model-runtime tests. Original/native
artifacts and these results are local evidence; exact-head Actions remain a
separate delivery gate.

## Native FPS, stack, widget and navigation gates

Four more fixed-input checker drivers are replaced by `check-ui-fps`,
`check-ui-stack`, `check-ui-widgets` and `check-ui-navigation`. Their independent
original corpora preserve 112 ordered FPS samples, 19 stack operations, 800
widget frames and 480 navigation frames. No candidate implementation generates
expected values. Original runtime/helper sources remain tracked and source pins
fail closed on drift; navigation also pins its NumPy dependency record.
These are Linux x86_64 baseline gates, not generic arbitrary-input helper
replacements, dynamic original recomputation or ARM numerical proof.

FPS and stack checks retain their distinct process/artifact semantics. FPS
inherits raw stderr and parses stdout before writing any of its three artifacts;
stack writes input/source before running the child and preserves raw native text
and the stderr fallback after successful strict decoding. Exact Python JSON
equality, nesting boundaries and the original FPS pretty-serializer limit remain.
Forty-two unchanged-original subprocess cases, eighteen CLI cases and explicit
source/depth tests pass. Both actual native examples match all eight output files
and stdout through original and native checkers. Physical raw-byte roots, binary
and output paths, inherited environment values and raw FPS stderr also match.

Widget comparison retains exact Python JSON equality. Navigation compares only
expected keys, including booleans and counts, with the unchanged strict absolute
bound `< 1e-6`; extra candidate keys remain admitted, and NaN remains rejected.
Forty-four original cases preserve malformed/mismatched output, child failures,
UTF-8/newline handling and partial artifacts. Source/dependency drift tests pass.
Fresh actual `widget_trace` and `navigation_trace` examples produce identical
three artifacts per checker and identical stdout/stderr through both drivers.
Raw path and symlink cases also match. Independent reviews found no Important
or Critical issues in either pair.

The shared framework still executes all thirteen original gates in order, now
with four native and nine Python commands. Caller regression tests execute the
orchestrator against controlled processes and retain invocation logs, complete
ledger values, display arguments, the existing EGL disk boundary and immediate
failure propagation. CI explicitly builds the five required UI checker binaries
including translations under its existing 512 MiB growth guard; it does not
build every dev-tools binary. All four replacements pass against actual examples
with `PATH=/nonexistent` after deleting their Python drivers. Eight UI caller
tests and Ruff pass. The source inventory now has 41 replaced and 3,539 pending
entries; no broader UI component is marked complete.

Current local validation for this four-checker unit passes 281 selected dev-tools
tests and 16 service-tools tests in the actual fast-checks routing. Strict
all-target dev-tools Clippy and package formatting pass; the eight UI caller
tests, Ruff and complete staged whitespace checks are separate integration checks.
Ongoing model compiler changes are excluded from this UI checkpoint.

The emoji checker remains pending after independent original probes identified
Pillow's acceptance of lossless TGA/TIFF/WebP and 16-bit PNG under `.png` filenames,
as well as mode-dependent NumPy broadcasting. A PNG-only replacement would remove
known behavior. Compact provenance and 21 observed cases are retained in
`rust/crates/dev-tools/tests/fixtures/ui-emoji/`; no codec dependency, Python
removal or format-equivalence claim was introduced for that unfinished unit.

## Native scroll and scroller gates

The two remaining headless scroll checker drivers are replaced by
`check-ui-scroll` and `check-ui-scroller`, preserving all 4,500 scroll frames
and 720 scroller frames. Source/dependency-pinned independent captures preserve
the exact discrete comparisons, strict scroll offset `< 1e-5` and velocity
`< 1e-8` bounds, and recursive scroller float `<= 1e-4` bound with original
key-set and sequence rules. Eighty-five unchanged-original cases include
boundary failures, boolean/numeric quirks, malformed streams, late failures,
stale outputs and CLI artifact phases; eighteen original CLI cases also pass.

The original DEBUG_SCROLL environment contract is preserved: only the exact
value `1` enables captured source debug output, emitted before each scene's
input/source files and child invocation. Partial output is retained on a failed
candidate. No child deadline or output limit was added. An explicit writer makes
phase ordering testable without mutating the process environment in parallel
tests. Full actual original/native runs compare every artifact and stdout byte
under DEBUG values 0, 1, 01 and a raw non-UTF-8 value, including raw paths and
inherited environment values. Sixteen actual comparison pairs pass; strict
focused Clippy and four native tests pass.

The framework retains all thirteen gates in order, now six native and seven
Python commands. Its regression suite includes immediate failure at both stack
and scroller, retaining command logs, ledger records, display arguments and the
EGL disk guard. Nine caller tests and Ruff pass. CI explicitly builds the two
new binaries under the existing UI-checker growth guard. After deleting the two
original drivers, both native checks pass against fresh actual examples with
`PATH=/nonexistent`. This does not retire the generic UI source modules or prove
ARM equivalence. Current source inventory: 43 replaced, 3,537 pending.

## Native diagnostics, communication snapshots and model timing

The three native commands `check-runtime-diagnostics`,
`check-communication-snapshot` and `check-model-timing` replace their Python
checker drivers together. Communication imported the diagnostic comparator;
that maintained dependency was migrated before either source file was removed.
The native comparator retains exact JSON types, object insertion order, integer
values and f64 bits, with the original equivalence for NaN payloads. It does not
reuse the looser UI numeric equality rules.

Independent full corpora preserve 18,314 diagnostic steps and 12,385 emissions,
11,205 communication snapshots with exact serialized cereal inputs, and 2,053
model-timing steps with 536 aggregates and the explicit original/native backend
name mapping. Ninety-two unchanged-original command cases and sixteen direct
scalar characterizations cover type/key-order/ULP/sign-zero differences, failed
children, strict stream decoding, JSONL separators, late failures and original
JSON nesting limits. Runtime, Communication and Timing retain their different
input/expected/actual artifact ordering, including serialization failures and
Timing's rejection of a pre-existing output directory.

Live runtime/source helpers, actual schema imports and applicable NumPy,
pycapnp, zstandard and pytest lock records remain pinned. Generic SubMaster,
message-state and runtime diagnostic helpers are not retired by this change.
The corpora are frozen independent Linux x86_64 original baselines, not dynamic
source recomputation or ARM evidence. Source or dependency changes fail closed.
Fresh actual native examples match all eighteen files and stdout/stderr through
both original and Rust checker commands. Six additional raw candidate/output
path and symlink pairs pass; no physical raw-repository-root equivalence is
claimed for pycapnp. All three commands also pass after source deletion with
`PATH=/nonexistent`.

CI explicitly builds and requires the three native commands, while retaining
all original model example construction and evidence publication rules. Its
new 512 MiB build allowance is tested separately from the unchanged 179 original
guards/hash and other added guard categories; the workflow now has 190 guards.
The source inventory has 46 replaced and 3,534 pending baseline entries.

## Bounded all-half Concat and driving preprocessing compilation

The next CPU compiler unit implements verified virtual and uint8-derived
all-half Concat paths and bounded direct finite initializer-vector reciprocal
folding. Independent original evidence showed that general physical-half Concat
movement and reciprocal optimization depend on scheduling context; those known
unsupported cases reject before publication rather than changing outputs or
relaxing comparisons. The final fixture has 239 small-graph equivalence positives
in both memory-planning modes and 218 separate rejection controls. Eight controls
use Neg outside the original offline loader's admitted operator set; rejected
original-valid cases are not counted as numerical equivalence passes.

A real driving preprocessing mismatch established constant-vector folding before
FDIV: 387,072 of 786,432 f32 patterns initially differed. Direct finite initializer
vectors of at most 4,096 values now preserve the original reciprocal-then-multiply
semantics. Computed constants, deeper dynamic arithmetic, special vectors and
broadcast reuse remain unsupported. The supported dynamic forms are direct
bindings through explicitly source-tagged non-growing views/casts and a proved
single Add of a normal nonzero initializer and such a binding. Every view edge
is checked; an Expand followed by Slice had repeated coordinates despite equal
endpoint cardinalities, so growth-bearing ancestry rejects. This is an explicit
partial admission boundary, not a complete optimizer or general injectivity proof.

True-final f32 exports of pinned driving-model prefixes 6, 7, 8 and 9 now match
independent original outputs exactly. The first Conv performs 113,246,208 products
and its f32 [1,64,64,128] output SHA-256 is
`621e5b7b9d987aa11a8c2902d918c9553cc49ab095c99ba4ccb82ee8ab92a435`.
The untouched driver model's existing 553-f32 whole-output gate remains exact.
Final reviewed model source passes all 151 package tests with no failures or
ignored tests, strict all-target Clippy and independent review. The genuine
A630 Add source and 2,144-byte binary remain byte-identical to the original,
and all seven source-proof hashes match current files. Earlier pre-repair test
runs are not substituted for this final result. Full driving-model, all-input,
GPU, device, compiler/training and Python retirement remain unfinished; no model
Python was deleted.

Combined local checks pass 290 selected dev-tools tests and 16 service-tools
tests through the actual fast-checks routing, all-target dev-tools/model Clippy,
workspace formatting, nine UI caller tests, Ruff and staged whitespace checks.
The separately run final model suite passes 151 tests. Exact-head remote Actions
remain a separate delivery gate.

## Outstanding delivery evidence

Checkpoint `30866c90dc246fddf2c963b3e5f0947390bee30d` was pushed to the
isolated issue branch. Its [Fast checks run](https://github.com/bin9208/openpilot-rust/actions/runs/38716577954)
failed in the logging-builder fixture: it froze the capture machine's unsorted
filesystem order for schema/generated files. The same failure was reproduced
locally on tmpfs. The repaired test uses the current directory enumeration order
while requiring the exact original file multiset, argument slots, fixed source
order and all other command/artifact fields. All five logging tests pass and the
eight-case original comparison also passes on tmpfs. Production enumeration is
unchanged; no source/flag checks were removed. The repair commit
`c8db3627a4d2296af01a5a9b4694a3f655061dad` passed its complete
[Fast checks run](https://github.com/bin9208/openpilot-rust/actions/runs/38717608255).
This is push-check evidence for that checkpoint, not PR/integration/device or
subsequent uncommitted source validation.

The next local checkpoint adds the hardware-command checker, firmware SPI
builder and bounded CPU math implementation. The actual current fast-checks
routing step passes all 136 selected dev-tools tests, 16 service-tools tests and
its remaining Python policy test. All 120 model-runtime tests pass; strict
all-target Clippy, workspace formatting and diff checks pass. Original/native
hardware fixture and SPI library comparisons are recorded above. These are local
results; the latest remote checkpoint `777bd1c7cd4830d28bd426154eb302d0e156a06e`
has passed push/PR fast checks, user docs and integration, and its complete [Rust workspace/aarch64 run](https://github.com/bin9208/openpilot-rust/actions/runs/38719609086) passed.
Checkpoint `7668b660bc89da7cb605878d574c5d97f340bcd1` was subsequently pushed;
its push/PR fast checks, user docs and integration pass, and its complete
[Rust workspace/aarch64 run](https://github.com/bin9208/openpilot-rust/actions/runs/38729020029)
also passed. No later commit inherits earlier CI evidence.

Checkpoint `a4fabd5637202905e5643057aadb1dd7f5ef4659` subsequently passed
push/PR fast checks, mapped user docs, integration, and its complete
[Rust workspace/aarch64 run](https://github.com/bin9208/openpilot-rust/actions/runs/38744711057).
The next verified checkpoint `38a5abc6a1e3bb0d7e3ffe7729b7fc62cc92d75c` is
pushed to the same isolated issue branch with 41 replaced source files. Its
exact-head Actions are pending separately; it does not inherit the earlier
success. In-progress all-half driving-model compilation and subsequent checker
ports are not part of that published checkpoint.

Continue the [implementation plan](rust-repository-python-retirement-296-plan.md)
through every remaining source family, actual consumer migration, a zero-Python
execution audit, independent review, exact-commit PR and merged-dev checks,
source-bound packaging and applicable device gates. Current local results are
not Actions evidence and do not establish repository-wide completion or CPU
savings. Issues #296 and #1 remain open.

Docs-Not-Needed: internal implementation and validation records; no setting or
user-guide behavior change.

## Native embedded Cargo test-artifact listing (2026-10-16)

The Radar ARM workflow now uses `list-cargo-tests` instead of its embedded Python
JSONL expression. This is a fourth retired embedded command, separate from the
46 retired source files. The native command preserves eager JSON decoding before
selection, ordered duplicates, Python truthiness and short-circuit lookup errors,
string-only selected executables, UTF-8/universal-newline input, Linux
surrogateescape output, and the independently observed decoder nesting limit.
Empty selection still emits a newline; the existing workflow nonempty-file
check, per-artifact execution/hashes and always-published evidence remain.

The unchanged original expression was run independently on 57 cases. Every
native API and actual CLI result matches original acceptance and stdout bytes,
including malformed records and later JSON failures that must publish no earlier
path. The native CLI runs with an empty PATH. A fresh Cargo service-tools
`--no-run --message-format=json` stream selected the same four executables;
original/native stdout SHA-256 is
`73d47bef87e5e649c79c8d2ac76d08febd614bfc2fd8c316e720f61d2490885e`.
The new native build has its own 512 MiB growth guard. All 179 original ordinary
reserve invocations remain unchanged. Full integration checks for this pending
checkpoint remain a separate gate.

The program text, SHA-256, interpreter version and all independent cases are in
`rust/crates/dev-tools/tests/fixtures/ci-artifact-list/`. Scratch captures and
RED/GREEN logs are under `.analysis/scratch/2026-10-16-ci-artifact-list/`.
The separate lazy recovery selector and other inline interpreter commands remain
pending; this change does not retire those or the generic Cargo toolchain.

## Native message-state CLI with retained shared helpers (2026-10-16)

`check-message-state` replaces the complete message-state checker CLI. Its
independent original corpus contains 11,205 SubMaster updates with actual cereal
message bytes. Forty-one complete original CLI cases pass, preserving 1e-12
inclusive float error, Python numeric equality, key-order independence, the real
maximum error report, all five original outcome assertions, child-owned output,
raw inherited stdin/stdout/stderr, and strict UTF-8/universal-newline reads.

Review found and fixed two meaningful parity defects before migration: the
original rereads expected.jsonl after the child, and its report counts those
post-child paired lines independently of the original generation outcomes.
Matched zero/one/extra-line edits therefore retain their original acceptance and
report counts. Both files are opened before decoding either, matching original
failure ordering. The final 41-case suite, focused outcome assertions and strict
Clippy pass; fresh real state_probe comparisons match all four files and stdout/
stderr for normal paths, raw-byte paths and symlinks. Actual CLI probes also
preserve raw FF child streams and 5e-13/exact 1e-12 error reports. Receipts are in
`tests/fixtures/message-state/actual-proof.json`.

Twenty maintained files still import source()/compare(). Those two function
source fragments remain byte-identical to the independent capture; only the
unused fixture generator/main CLI and their unused imports were removed.
The remaining Python module explicitly fails direct invocation with the native
command name. Its live hash is
`928e7cf946a1fbf3c6c0bc9135c580e8f108179cfe78349956eb895f6d0c710a`;
original whole-file/function hashes remain immutable provenance. The messaging
and communication guards now pin that remaining helper while preserving every
other source/schema/package guard. A direct import, SubMaster update and comparator
smoke check pass after trimming. The file remains **pending** in the inventory;
this is not generic helper retirement and does not increase the 46-file count.

The workflow builds the new CLI under a separate 512 MiB growth reservation,
keeps the service catalog check and message-state artifact publication, and
calls the native checker. Fast routing covers caller/build/evidence selection;
the full 41-case suite remains mandatory in workspace tests. Post-migration
integration validation is recorded separately before the next checkpoint.

## Independent native EGL reference and checker (2026-10-16)

`check-ui-egl` and its fresh `ui-egl-reference` child replace the original checker
and helper. The reference policy is Rust and independently implements the source
initialization, symbol fallback, retry, NV12 attribute and descriptor behavior.
A separate raw ABI adapter uses the existing audited startup-ui CXX boundary for
individual typed library calls and checked opaque handles. It never calls the
candidate EGL lifecycle implementation. Generic runtime `egl.py` remains pending.

A fixed canonical trace was insufficient: the original keeps an existing GLES
library, including valid alternatives and a preexisting invalid file. The native
reference therefore executes the actual supplied private libraries. Independent
proof includes 56 original executions (seven baseline, 35 ABI variants, 14 cases
where the foreign library closes a descriptor). Partial EGL library ownership
survives a failed GLES load through the final FD count. Review exposed a real
native SIGABRT when the foreign library had already closed a duplicate; a checked
ownership-transfer close now preserves the original distinction between ignored
destroy-close errors and propagated rejected-create close errors, without unsafe
Rust or public arbitrary-FD close operations.

Six focused tests and strict feature-enabled Clippy pass, including unknown/stale
handle and malformed attribute rejection. Forty-one whole original checker cases
and nine CLI captures preserve compile logs, relative symlinks, streams, error
artifacts, delayed assertions and JSON limits. Twelve whole original/native runs
using fresh real compiler output and the current native candidate match all 46
artifacts plus stdout/stderr byte-for-byte. They cover borrowed/owned modes,
canonical/existing-invalid/alternate-valid GLES, raw paths and raw inherited
environment. Two native runs pass with only a compiler shim on PATH. Immutable
source provenance and receipts are in `tests/fixtures/ui-egl/`; retained egl.py
and complete cffi/pycparser lock records have a fail-closed drift guard.

The shared UI caller now selects the native checker while retaining its 26 GiB
fixture admission, ordered logs, per-child ledger and first-failure exit. Ten
caller tests pass, including failure at the final EGL stage. The UI job explicitly
builds both commands and runs both optional-feature test targets under the pinned
raylib environment. Default dev-tools builds remain independent of raylib. The
native UI checker build reserve is increased to 2 GiB for the new dependency
closure, with a separate 512 MiB test-build guard; all 179 original ordinary
reserve invocations remain unchanged.

The two replaced Python files were removed after actual comparison and independent
review. Inventory now records **48 retired source files / 3,532 pending**, plus
four separately retired embedded commands. This host evidence does not establish
ARM, real display or vehicle validation. The remaining UI runtime and orchestration
families, model compiler and repository-wide conversion remain open.

## Native Bluetooth command-reader comparison gates (2026-10-16)

`check-command-reference` and `check-command-json-boundary` replace their two
Python CLI drivers. The independent source corpus preserves 1,026 ordered reads,
381 returned commands and all 23 JSON-boundary cases. Native replay retains every
source journal/request write and its order; the candidate still owns its separate
journal directory. Generic Bluetooth model.py and arbitrary runtime behavior remain
pending, guarded by original model/package hashes and namespace-package absence.

Sixty-nine whole original cases and 18 CLI cases pass, preserving Python JSON
numeric equality, lone-surrogate handling, malformed/deep JSON behavior, raw
inherited streams and cached expected values. The boundary gate rereads source
and binary hashes after the child, preserves the imported source-root symlink
alias, writes/prints the difference receipt before failing, and retains the
original JSON serialization failure phase. A child editing reference.json does
not replace the reference checker's in-memory expectations.

Strict Clippy and six real original/native comparisons with fresh command_probe
pass. All files, directories, stdout and stderr match for both gates across normal
paths, raw byte paths and an ASCII alias to a raw source root. Both native commands
run with PATH=/nonexistent; repeated checks still pass after the Python drivers
were removed (1,026 reads/381 commands and 23 cases with no differences).
Provenance, corpus integrity and actual receipts are in
`tests/fixtures/command-reference/`.

The workspace gate now builds both native checkers under a 512 MiB growth guard,
retains the preceding desire-state check and existing command report publication,
and runs both native consumers. The full fixed-case tests are added to fast
routing. This raises the source inventory to **50 retired / 3,530 pending**;
four retired embedded commands remain counted separately. Generic Bluetooth,
desire-state tooling and repository-wide completion are still open.

## Dynamic native Params interoperability checker (2026-10-16)

`check-params-reference` replaces its Python driver while still compiling and
executing the unchanged C++ Params implementation against the current Rust store.
The catalog is dynamic; no expected key snapshot replaces the original oracle.
Fresh normal/raw-path original/native runs match all stdout/stderr for 367 keys,
both-direction 256 KiB binary and empty writes, remove/clear behavior and both
real advisory-lock checks. The native-only PATH contains compiler tools but no
Python, and each isolated temporary parent is empty afterward. A repeat after
Python deletion passes the same gate.

Twenty-two whole original cases, ten copy-failure cases, metadata/umask captures,
ten CLI cases, temporary fallback captures and cleanup behavior informed eight
integration tests plus two cleanup unit tests. All pass with strict Clippy.
Review repairs preserve source-open/target-open/stream-copy order and empty partial
files, copyfile versus copy2 mode/time/xattr semantics, source symlink handling,
continued sibling copies before aggregate failure, path-based utime for write-only
files, original temporary-root fallback, and Path.exists' ignored errno set.
Owned readonly cleanup never follows external symlinks. Cleanup errors override
body errors, and the successful PASS line remains observable before a subsequent
cleanup error, matching the original output phase.

One explicit cleanup correction is intentional: the original Popen context can
wait indefinitely after its ten-second post-unlock timeout. Independent evidence
records that source hang under an external harness deadline. The native tool keeps
the 0.1-second blocked probe and ten-second release wait, then kills/reaps its owned
child on failure. This is not reported as source exit-status equivalence. Ordinary
compiler/store operations retain their original unbounded timing behavior.

The current workflow builds the native checker with its own 512 MiB reservation;
real compiler calls also check their operation filesystem. Fast routing now includes
the complete focused Params tests. Developer reproduction instructions point to
the native command. The original Python driver was removed after independent review
and actual C++/Rust validation. Inventory is **51 retired / 3,529 pending**, with
four embedded commands retired separately. Linux host results do not establish
ARM/device acceptance or complete repository conversion.

### Native tools checkpoint validation

The final integrated fast-routing command passes 310 dev-tools tests and 16
service-tools tests. Six optional EGL feature tests pass. Default dev-tools
all-target Clippy and the explicit EGL-feature Clippy invocation pass with warnings
denied. Native-tool/startup-UI formatting, ten Python caller tests, Ruff and diff
whitespace checks pass. Inventory was reconciled against actual tracked source:
51 removed entries, 3,529 still-present pending entries, and no unindexed sources.
All 179 original ordinary disk guards remain unchanged; the current workflow has
195 guards including the independently admitted new build/test operations.

Remote commit `38a5abc6a1e3bb0d7e3ffe7729b7fc62cc92d75c` completed all 46 Rust CI
jobs successfully at
https://github.com/bin9208/openpilot-rust/actions/runs/38759226239 . Its Fast,
documentation and integration gates also passed. Those results do not validate
this subsequent tools checkpoint; its exact-head CI is a separate required gate.
The model compiler still has incomplete driving-model numerical/scheduling
coverage and remains a separate unfinished implementation unit. No model Python
was removed, and these tooling checks do not complete repository-wide conversion.

### Native Miri core-library hash collection

`hash-miri-core` replaces the embedded Python expression in the RadarCAN Miri
workflow. It scans the actual `MIRI_SYSROOT`, preserves pathlib's component
sorting, dot/parent/double-slash spelling, hidden entries, symlink traversal,
filesystem surrogate escapes and JSON formatting, and hashes every selected
library. Empty selection and any selected read failure prevent JSON publication.
Thirty independently captured original executions and the real existing Miri
sysroot match. The latter native command ran with PATH=/nonexistent and produced
stdout SHA-256 `1d88caa42ab506cdb3c5d1d7137837d5f70b4b35cd5f5a5115e2de2dc1f7e528`.

The workflow removes its now-unused Python setup, reserves an additional 512 MiB
before building the collector, and still runs all four original Miri levels and
both numerical/scheduler filters with artifact publication. No original disk
reservation was removed. This is the fifth replaced embedded command; the source
inventory remains 51 retired / 3,529 pending. Hash equality is not evidence of a
new Miri numerical run, complete tooling conversion or device acceptance.

Independent review found a valid mismatch before publication: directly opening
the literal `lib/rustlib` prefix skipped pathlib's directory listing of each
component, accepting an execute-only sysroot that the original rejected. The
replacement now reuses the audited Python glob selector. Five original permission
cases and a late unreadable-file case cover that repair; no expected result was
changed to match the replacement.

After the permission repair, 46 focused Miri/workflow/disk regression tests and
strict scoped Clippy pass. The actual sysroot comparison was repeated against the
current source-matched native binary and remains byte-identical. The workflow now
has 196 disk reservations, retaining all 179 original ordinary reservations.

A second review probe preserved absolute-root hashing from an already removed
working directory: the replacement no longer adds a `getcwd()` requirement.
Its independent original capture and native regression now pass, bringing focused
test coverage to 47 tests and original command scenarios to 31. The current
source-matched binary's real Miri hash output remains unchanged.

Final scoped Clippy passes with warnings denied after both review repairs. The
independent Miri review is closed with no remaining actionable findings.

### Native validated catalog generation selection

`model-run --catalog-generation ROOT` replaces the workflow's embedded Python
`current.json` read. It reuses `Catalog::load`, retains the verified index identity
and prints it without loading executable kernels. The existing `--check-catalog`
output remains unchanged. The workflow still builds the tool first, validates the
whole catalog and runs both camera-resolution publication comparisons.

Independent original Python output and complete non-executable catalog metadata
are retained for x86_64 and aarch64. The host CLI matches the original generation
bytes with Python absent from PATH; the fixture for the executing architecture is
selected by the test. A tampered index produces no successful metadata output.
Repeating validation intentionally prevents stale/tampered catalog metadata use;
this command is not a general replacement for Python printing arbitrary JSON
values. The previous workflow already required a valid catalog before this step.

Four catalog tests and 32 workflow-isolation tests pass. Both focused model-runtime
and dev-tools Clippy invocations pass with warnings denied, and independent review
is clear. This is the sixth embedded Python command replaced; baseline source
retirement remains 51 / 3,529 pending. No full model compiler, Python removal,
training, GPU or device completion is implied by metadata-only validation.

### Native image-array comparison foundation

`image_array_math::compare` matches 97 independently captured NumPy 2.5.3 cases:
91 successful comparisons and six shape/empty/dimension failures. Dense i64 input
samples retain trailing-axis broadcasting, last-axis any/sum grouping, and
wrapping subtraction/absolute value, including negative INT64_MIN differences.
The native implementation streams coordinates instead of allocating a broadcast
or difference image. Its source-corpus test and strict scoped Clippy pass;
independent read-only review is clear. Fast routing includes this regression.

This is a comparison primitive only. Image decoding/modes, original pixel-warning
thresholds and full checker/caller migration remain pending. No additional Python
source is retired by this helper.

### 2026-10-18 resumed native amplifier policy gate

Work continues solely on `codex/feat-296-repository-python-retirement` from
stable dev; the primary checkout's concurrent dev-md work is not an input.
User reconfirmed complete tool conversion and final offroad boot/recording/upload.
USB and ADB inventories currently expose no comma device; reconnection requested.

Captured all 2,034 unchanged original amplifier cases, exact inputs/outputs and
both source hashes independently. Native fixed-input checker validates corpus
integrity, still-live amplifier source and the explicitly retired checker digest.
Its 6,103 generated files (including result.json) match the original byte for
byte using the same actual native amplifier example. Repeated after removing
`check_amplifier.py` with PATH=/nonexistent: PASS and empty recursive diff.
CI hardware gate now calls the native checker. The generic amplifier source and
Linux syscall reference remain separate work, not replaced by fixed responses.

Independent review caught depth-limit bypass by overwritten duplicate JSON keys,
missing checker-source drift enforcement and argparse error exit mismatch. All
three reproduced RED and their repair passed five focused tests. Eight original
decoder boundary cases (depth 9994–10001) pass the unit guard. Further review found
UTF-16 byte autodetection on already decoded text: corrected regression reproduced
2,034 artifact directories instead of zero. Raw-NUL rejection is implemented;
final regression/Clippy after that last repair remains pending the coordinated
Cargo slot. A test peer's serde default float round-trip was also corrected to
preserve original JSON decimal lexemes; original expectations did not change.

Model LLVM18 materialized IMAGE boundary passed 36 exact output-byte cases and
12 schedule captures (188.70 s). Varied-weight/extended-channel controls, provider
20.1, complete model packing/ABI integration and the earlier C shortcut are still
pending. This is not full-model completion. Image-array package tests now pass
captured modes, malformed cases, thresholds and JPEG acceptance; full installed
Pillow format admission/native codec staging remains ongoing. Neither partial
area authorizes deleting model or image-checker Python yet.

Hardware-control checker next unit captured 35 original fixed scenarios and 14
success/failure/encoding artifact-order cases. The new stub test failed RED as
expected; implementation and final regressions are in progress. An attempted
actual comparison using the shared old host cache rejected `openpilot-reboot`
versus stable source `sudo reboot`. The selected #296 source still correctly
contains `sudo reboot`; this identifies an unrelated cached executable, not a
#296 source regression. Rebuild both amplifier and hardware-control examples
from this worktree into the dedicated retirement cache before final proof.
Do not use the shared primary/dev-md target as provenance for #296 executables.

Latest parent gates: 6 amplifier, 2 shared comparator, 32 CI routing and 1 hardware
policy test passed (41 total), plus strict focused Clippy. The last text-admission
fix is now verified. Dedicated-cache source-matched examples were rebuilt:
amplifier SHA256 `f00cc8c881dfaa0663fc54a0c599b776f99bebd187c87622dac2a117d220ff06`,
hardware-control `79dd686ee09caf7237f1c02885da0ce972a57e55d33dd04211f9dc269cbd1547`.
Original/native amplifier 6,103 files and hardware-control 38 files match byte for
byte. Both native checks pass with PATH=/nonexistent, after deleting their original
fixed checkers; generic runtime/reference sources remain pinned and retained.
Independent hardware review regenerated all 35 cases with zero mismatches.

Retirement inventory is now 53 replaced-local / 3,527 pending baseline entries.
Both native tools are connected to fast and hardware-runtime CI. Added later
hardware live-source drift regression and Linux amplifier reference integration
remain awaiting the next Cargo slot; this is local work, not new exact-SHA CI or
completion. No device boot/recording/upload result is claimed.

User update (2026-10-18): comma is unavailable; the user will git-pull and perform
normal boot/recording/upload acceptance personally. Do not request reconnection
or wait for device access. Continue code conversion, optimization, test gates and
reviewed commits/integration to dev; record physical acceptance as user-owned.
This supersedes the earlier device-reconnection request for this continuation.

The final parent regression run after hardware source-drift tests passes all
6 amplifier, 3 hardware-control and 32 CI-isolation cases (41 tests). This
includes changed reintroduced generators, every live hardware policy dependency,
pre-existing evidence preservation, malformed text and original failure artifact
ordering. The two fixed policy checkers are ready to commit independently of
unfinished image/model compiler work. Generic source helpers remain retained.

Native Linux-amplifier and polygon reference integration is still under review.
Their earlier 27 fault/17 generic and 76 scene/14 path/12 warning comparisons
are bounded evidence, not authorization to remove unrelated Python sources.

### Independent Linux I2C reference and live comparison retirement

The Rust source reference owns arbitrary JSON byte-read/write actions, conversion,
forced-address order, error propagation and context-close precedence. Its small
C child performs only Linux open/ioctl/close, using the original selected-file
preload fixture; it does not reuse the candidate amplifier implementation.
The original generic I2C policy and fixture remain live SHA drift dependencies.
Both retired tool identities and the complete original27 capture are pinned.

All 27 original fault scenarios, 17 generic inputs, six pre-open errors, six CLI
cases and 13 trace separators are retained. Independent review reproduced and
repaired empty --case selection and missing-input initial-register persistence.
Two unit and seven integration tests plus strict scoped Clippy pass. The fresh
live source/candidate gate passes all 27 cases with empty --case, and 483 artifacts
match after output-root normalization; all 27 changed source invocation receipts
separately assert the actual Rust owner environment and raw child preload.
Repeated after Python deletion with compiler-only PATH: all 27 PASS.

CI now invokes check-amplifier-linux; the two Python tools are removed. Baseline
inventory is 55 replaced-local / 3,525 pending. This remains an independent host
I2C fixture comparison, not hardware/device acceptance or complete Python removal.

### Native statsd numeric aggregation checker

All 598 original fixed-seed numeric cases were independently captured against
unchanged statsd aggregation, preserving the original Python float text, signed
zero, nonfinite values and exact output strings. Thirteen original process-result
scenarios retain failure/stdout/report-write ordering. Three native tests cover
those cases, source drift/reintroduction, pathlib runner normalization and missing
report parents. Stub RED was observed before implementation; all three tests and
32 routing tests pass, along with strict scoped Clippy. Independent review
regenerated every frame/expected string and all 13 error captures without finding
an additional issue.

The dedicated-cache stats_trace was rebuilt from #296. Original/native checker
reports match byte-for-byte across all 598 cases. Repeating after Python deletion
with PATH=/nonexistent also passes with an identical report. The telemetry job
builds and invokes the native checker; fast checks include its regressions.
Inventory is now 56 replaced-local / 3,524 pending. Generic statsd and other
numeric tools remain explicitly separate; no full-project completion is claimed.

### Native statsd clock checker

The fixed CPython 3.12 oracle now runs as `check-stats-clock`: 5,048 calendar
rounding, 5,009 monotonic bit-conversion and two rejected calendar-boundary
inputs are independently captured with source hashes. The native checker also
preserves 25 success/failure sequences and 38 Python integer-admission cases,
including arbitrary integers, Unicode digits, line separators, output ordering
and pre-existing report preservation. The stub failed its integration regression
before implementation; four native tests, strict scoped Clippy and formatting
pass. The existing native arbitrary integer parser is reused.

A freshly rebuilt #296 stats_clock example passed both original and native
checkers with byte-identical 598-byte reports and stdout. The parent repeated
the native gate after deleting the Python checker, with PATH=/nonexistent in
the actual repository; all 10,057 normal inputs and both boundaries pass and
its report still matches. The telemetry workflow now invokes the native binary
and fast checks include its four regressions. Inventory: 57 replaced-local /
3,523 pending baseline sources. Full Python retirement and user-owned device
acceptance remain open.

Concurrent SDK review reproduced reuse of existing work/install directories,
which could retain an old successful receipt after a failed attempt. The pending
image stager now requires fresh separate destinations, checks disk space for both
filesystems and validates library receipts. Five standalone tests pass, including
stale directories, dangling links, overlapping paths and mutated SDK libraries.
The new full SDK build and image-format completion are still pending; these
uncommitted image/model changes are excluded from the clock retirement commit.

### Exact-head CI findings and follow-up

PR head `8174191e9fee1d90fc128f482226d56f1ce6224a` passed Integration
[39055899763](https://github.com/bin9208/openpilot-rust/actions/runs/39055899763)
and mapped user docs. Both fast-check runs reached the seven-minute limit after
all native/navigation/Jetlink tests executed so far had passed; Android had not
run. Cancellation is not a successful gate. The follow-up separates independent
native and Python/Android workers and keeps an always-run `fast checks` aggregate
that requires both results. Every original test command remains present.

The same head's Rust workspace job
[117463838960](https://github.com/bin9208/openpilot-rust/actions/runs/39055899612/job/117463838960)
failed rustfmt at the Miri provenance glob call before workspace testing.
`2f18ac50` applies the exact Rust 1.94.0 formatting correction. Later source and
CI fixes require fresh exact-head runs; prior successes do not clear them.

Clock checker follow-up: importing carrot-navi solely for integer normalization
would also require its runtime/schema build graph in the minimal CI bootstrap.
A dependency-graph regression reproduced that chain, then passed after replacing
arithmetic with equivalent canonical decimal text. The existing lightweight
Unicode decimal table is reused. Independent CPython evidence covers all 680
Unicode 15.0 decimal digits, all other Unicode scalars, 29 whitespace classes,
the prior 38 lexical cases and the same 4,300-digit limit. The final lightweight
checker again matches original report/stdout bytes without Python. Five clock,
five workflow, 32 Rust-isolation and 13 disk-policy integrations all pass
(55 unfiltered tests), plus the exhaustive Unicode unit and strict Clippy.
Parallel fast-check gate mutations and actual shell tests reject failed,
cancelled, skipped, empty or missing worker results. Seven-minute worker limits
and every prior test command are preserved.

### ARM planner IPC replay pacing and exact-head follow-up (2026-10-18)

Head `a560c4613113754007d96d5fbb8c9b00b974eb16` passed both fast-check
runs [39057569430](https://github.com/bin9208/openpilot-rust/actions/runs/39057569430)
and [39057564827](https://github.com/bin9208/openpilot-rust/actions/runs/39057564827),
Integration [39057569320](https://github.com/bin9208/openpilot-rust/actions/runs/39057569320)
and mapped user docs. Its Rust job remains unsuccessful: workspace Clippy found
`items_after_test_module` in the stats clock checker, and ARM planner IPC failed.
The test module has been moved after production items; a fresh Clippy gate remains
required for that correction.

The ARM artifact identifies the mismatch between two original Python executions,
not between original and Rust: original epoch 1 reports false for the second
lateralPlan/driverAssistance publication, while original epoch 0 and both Rust
epochs report true. All four runs retain 121 messages, six model triggers,
35 liveTracks triggers and 34 fast overlays. Original epoch 1 first processing
completed at 448.556855263 after a nominal 448.531671547 deadline; the fixed next
448.581671547 send left only 24.816284 ms after completion. Original
FrequencyTracker checks receive intervals, so replaying fixed deadlines can turn
startup processing delay into an artificial burst.

The IPC harness now schedules the next model batch at least 50 ms after the
previous model completion, preserving all packets, comparisons, invalid-input
cases and runtime validity policy. Its regression evaluates the actual scheduling
expression with the recorded ARM values: the old expression fails, the new one
passes; all four planner CI tests pass. Real ARM IPC rerun remains required.
Failure evidence: [job 117468881364](https://github.com/bin9208/openpilot-rust/actions/runs/39057569274/job/117468881364),
artifact 12110875280. No runtime threshold or output assertion was weakened.

### Native beep clock checker (2026-10-18)

The beep monotonic gate now uses `check-beep-clock` with 1,011 independently
captured CPython 3.12 time conversions. Fifteen original child-result scenarios,
seven CLI cases and three artifact-write failures preserve admission, output and
failure ordering. Four native integration tests pass. The shared stats-clock
integer parser is reused, and its test module was moved after production items
to resolve the exact-head Clippy failure without changing conversion behavior.

The current #296 `beep_trace` example was rebuilt. Original and native checkers
produce byte-identical stdout and all three artifacts. After removing the
original Python file, the actual native command again passes all 1,011 inputs
with PATH=/nonexistent and byte-identical evidence. The parent verification also
passed 76 tests across ten affected integration binaries and strict dev-tools
Clippy with polygon-reference enabled; this does not clear subsequent image
review findings or constitute device validation. CI now calls the native beep
clock tool and includes its regression tests. Inventory: 58 replaced-local /
3,522 pending baseline sources. Full retirement and user-owned device acceptance
remain open.

### Pinned LLVM worker and packed IMAGE compiler integration (2026-10-18)

The native compiler now isolates explicitly pinned LLVM providers in a bounded
worker, verifies source/object/optimized-IR receipts and rolls back failed
outputs. Selected Conv/residual IMAGE closures use the original packed-half
layout and reduction scheduling, with checked argument lengths, disjoint views
and 32-byte alignment before LLVM alias/alignment promises. Original held raw
outputs remain observable. Provider failure cannot silently fall back to C.
The former aligned-channel accumulator shortcut is replaced by the source affine
reduction-axis rule, retaining loop identity and signed-zero/NaN behavior.

Compiler process-group cleanup now observes an exited leader with WNOWAIT,
terminates its descendants while the leader still reserves the group identity,
then reaps it. Normal-exit and timeout lifecycle tests and independent syscall
ordering checks pass; cleanup never signals the group after losing ownership.

Latest source passed the complete LLVM18 model-runtime all-target suite: 178
checks across 28 binaries, none failed or ignored. The affected LLVM20.1 suites
passed all 16 checks, including 48 complete graph compilations/executions with
both memory plans and 12 held-output cases. Strict all-target Clippy passed.
A newly built aarch64 offline QCOM helper running under QEMU reproduced the
original A630 OpenCL source and all 2,144 binary bytes. The helper SHA256 is
1473639dd5e6eb6e3ac260380353fa6ea7642be08da3bfd7553f2ae09eab42a8;
this is host/QEMU compiler evidence, not physical target execution.
CI stages the exact independently captured LLVM18 library with archive and
library hash verification; no uncaptured provider or skipped numerical check is
accepted. Independent source review found no remaining issue in this bounded
implementation; evidence is indexed in the compiler fixture README.

Full compiler conversion remains open. A fresh full-driving admission probe
rejects node274 `policy/node_MatMul_13`, shape [1,9,512] x [512,512], at the
rank-two IMAGE matrix boundary without publishing an artifact. Batched packing,
general normalization, other scheduling/GPU/training paths and caller migration
still require their own original comparisons. No model Python files are removed
by this change and this result is not a full-model parity claim.

### Polygon/image and beep sleep/integer retirement (2026-10-18)

Four more original Python sources are retired after production caller migration:
check_ui_polygon.py, ui_qa/polygon_source.py, check_beepd_sleep.py and
check_beepd_integer.py. Inventory is 62 replaced-local / 3,518 pending baseline
sources; this is not whole-repository completion. Device boot, recording and
upload are user-owned under the latest instruction; no reconnection is required.

The independent Rust polygon scene reference preserves editable scenes and raw
raylib behavior. The checker retains raw Pillow/NumPy mode, shape, broadcasting,
integer conversion and threshold semantics across the captured image formats,
including files with non-PNG contents named .png. Nine source-pinned native codec
libraries are provisioned without Python/Meson build tools; production CI requires
the full decoder feature, SDK tests and Ghostscript for EPS. Optional default
workspace builds do not require that SDK.

Independent review exposed and repaired TGA/CUR dispatch, EPS BoundingBox and
rendered-mode precedence, FITS byte-card parsing, short data-record offsets and
compressed one-axis storage behavior. All 85 independently captured regression
cases now pass. Fixture audit verifies 971 encoded inputs, 92 checker blobs and
443 independent native expected arrays. Current CLI after source deletion passes
all seven production scenes, seven TGA collision scenes, 76 arbitrary original
scenes and six boundary cases with no interpreter execve observed. None of these
host comparisons establishes device rendering or exhaustive codec equivalence.

The sleep checker matches all 27 original conversions and exact report/stdout,
including after source deletion with PATH=/nonexistent. Five tests preserve 35
original child-result failures, JSON depth and artifact write order, two filesystem
faults and seven CLI captures. The integer checker now invokes a live, source-bound
C++ Params helper instead of the Cython binding. All 278 source results match the
independent original Cython captures exactly, including stdout/stderr and 245 real
SIGABRT exits. The actual Rust beep candidate also passes all 278. Retired-source
PATH=/nonexistent output and summary match the source-present run byte-for-byte.
The explicit provenance lane changes from python to source-reference; the shared
Cython helpers remain for their other unconverted callers.

Final parent gate passes 78 tests across ten affected dev-tools binaries, five
native-codec tests, ten JPEG checks, ten qcomgpsd tests and strict dev-tools/qcomgpsd
Clippy. The formatting correction passes a subsequent workspace fmt check.
CI mutation tests require live integer helper build/check in order and reject
missing, conditional, softened or old-Python routes. Final exact-head remote CI
is still required; earlier commits' results cannot substitute for these changes.

### Shared Ratekeeper native checker (2026-10-18)

The fixed selfdrived timing comparator is replaced by check-ratekeeper. The
unmodified original generated all 20,000 Ratekeeper/MovingAverage states,
including every 100-slot history buffer. Those complete source JSON lines and
inputs are pinned and compressed; no replacement output supplies expected values.
Runtime realtime.py/utils.py remain required live drift guards. Only the fixed
checker is retired; generic Python timing source remains pending.

Actual original/native comparison has byte-identical input.jsonl, source.jsonl,
native.jsonl, native.stderr, manifest.json and stdout. Repeating the native CLI
after deletion with PATH=/nonexistent produces the same artifacts. Seven native
checker/routing tests pass, preserving 16 failure cases, three artifact-write
faults, six CLI cases, live-source absence and changed reintroduced-source
rejection. Strict scoped Clippy passes after simplifying a nested CI-test guard.
The full corpus regression takes about 111 seconds locally; its coverage must
run in a separate required worker instead of overloading the old seven-minute
native lane. Inventory: 63 replaced-local / 3,517 pending baseline sources.

Published9d229850 passed fast PR checks and mapped docs; its UI runtime CI failed
closed while checking a libyuv archive hash. Googlesource generates timestamped
archives; source-identical, stable full-commit mirror packaging is under repair.
Other ongoing CI results and user-owned device checks are not presumed passed.

### Source archive reproducibility and mandatory fast-corpus split

Commit31f40181 fixes the libyuv download failure without changing its source.
The stable full-commit mirror archive contains the same 204 paths, file bytes,
links and executable bits; its reconstructed Git tree is
ba22b636166dc16532fcc9d5a9524117432d1782, matching both origins for
644251f252a84bf8ce91ff0aca86a9b16b069ab8. Repeated downloads match the pinned
archive SHA. All nine fresh archive downloads, fresh SDK build, six stager tests,
five native codec tests, sixteen image/polygon/CI tests and strict scoped Clippy
pass. The extraction regression requires stripping exactly the mirror root.

The published9d229850 fast push worker reached its seven-minute cancellation
boundary although the PR fast worker passed. Native routing and behavioral
corpora now run as separate mandatory workers; all 61 existing/current native
integration targets remain exactly once, plus library tests, Python CI tests and
service-tools. The aggregate checks actual success from all three workers,
rejecting missing, skipped, cancelled, timed-out or failed results. Seven-minute
worker bounds are retained. This is CI scheduling, not removed test coverage;
new exact-head remote success remains required.

### Cut-in audio-alert validation script retirement (2026-10-18)

check-cutin-alert replaces the fixed Python checker using all 5,786 independently
captured source states and exact float-bit inputs, including its 5,000-frame
randomized suffix. Seventeen failure/process cases, six CLI cases, artifact
ordering, live-source drift, source absence and all original equality semantics
remain checked. Generic runtime CutinAlertTracker/promoted-candidate source stays
live; no detection, lead selection or NAS replay policy changed.

The actual source/native binaries produce byte-identical input.jsonl, source.jsonl,
native.jsonl, native.stderr, report.json and stdout. After checker removal,
PATH=/nonexistent repeats the same result. Parent validation passes all 28 affected
tests (13 disk bootstrap, six workflow policy, three checker, three routing and
three Ratekeeper routing), scoped strict Clippy and the actual example build.
The six cut-in tests pass again after deletion. CI now selects both native gates
with explicit disk guards and keeps all nine surrounding source comparisons;
all 63 native integration targets remain mandatory exactly once.

Inventory: 64 replaced-local / 3,516 pending. Published3c194582 fast PR gate
passed, but its workspace job117482992011 failed formatting at the newly added
Ratekeeper module declaration. The declaration is now rustfmt-sorted. Current
uncommitted model work remains separately owned and is not part of this commit.
Full exact-head CI and the user-owned device checks are still outstanding.

### Selfdrived state-machine checker retirement (2026-10-18)

check-selfdrive-state retains all 51,023 independently captured original steps:
30,720 exhaustive single-step combinations, the full soft-disable transition and
20,000 deterministic sequence steps. Exact input/source bytes, 17 process/failure
captures, artifact failures, CLI and all three live-source pins are retained.
No state transition, event priority, timing constant or validity policy changes.

All four checker tests pass before and after deleting the original checker;
25 affected disk/routing/workflow tests and scoped strict Clippy also pass.
The actual state-machine example builds and matches the original checker with
byte-identical input.jsonl, source.json, native.stdout, native.stderr, result.json
and command stdout/stderr. PATH=/nonexistent after deletion repeats those bytes.
This fixed comparison does not replace the generic state.py/events.py/realtime.py
implementations, which remain live source-drift guards and pending conversion.

CI selects the native state tool with its own 2 GiB growth guard. Mandatory fast
corpus coverage grows to 64 total integration targets across the two native jobs;
source checks surrounding this conversion remain in their original order.
Inventory: 65 replaced-local / 3,515 pending baseline sources. Published3c194582
PR and push fast gates, Integration and mapped-doc checks passed; its separate
Rust workspace formatting failure is corrected by c2dcf2ea. Later exact-head
checks remain required. No device is connected or requested; user owns final
boot/recording/upload checks after source integration.

### Complete alert/event/helper checker family (2026-10-18)

Native check-alert-manager, check-selfdrive-events, check-alert-callbacks and
check-selfdrive-helpers replace four fixed Python comparison commands. Independent
unmodified CPython captures retain all 25,000 manager, 4,872 event, 15,876 callback
and 10,241 helper cases, including exact requests, expected policy results and
failure artifacts. Generic runtime policy/schema files remain mandatory live
source guards; the helper also binds NumPy/pycapnp lock records. No runtime alert,
pose validity, actuation threshold, timing or vehicle-control policy changed.

All 17 checker integration tests pass. Independent review reproduced one callback
surrogate-encoding discrepancy: original Python leaves empty differences.json;
the initial native version left no file. The added original capture first failed
and then passed after preserving the encoding-time truncation boundary. Helper
lock mutation initially changed a transitive reference instead of the selected
package record; correcting the test target makes the actual package-drift guard
pass without changing product validation or deleting coverage.

Immutable embedded alert corpora now decode/validate once per process. Live file
hashes, retired-source reintroduction checks and the translation set still run on
every call. All manager cases and error/output checks remain: the same local suite
fell from 251.86 to 165.17 seconds (other local work was active, so this is not a
controlled benchmark). Callback/event suites took 17.11/13.30 seconds; helper took
37.52 seconds. This is developer-test cost evidence, not runtime/device CPU data.

Actual current selfdrived examples build and pass both original and native checkers.
All result artifacts and stdout/stderr are byte-identical, except callback source
exception stacks whose ASLR addresses are normalized while retaining all exception
classes/messages/rows. After deleting the four Python checkers, PATH=/nonexistent
runs reproduce every native artifact and stdout/stderr byte exactly. Native manager
comparison took 32.21s versus original7.12s locally; language replacement itself is
not a speedup claim. Local proof: 2026-10-18-selfdrive-alert-family and
2026-10-18-selfdrive-helpers scratch directories. Original callback surrogate and
all failure captures are tracked with provenance.

CI now requires a separate seven-minute alert corpus worker and includes every
new integration target. The aggregate rejects missing/failed/cancelled/skipped
workers. Runtime comparisons select the four native commands with explicit growth
checks, preserving surrounding source comparisons. Inventory:69 replaced-local /
3,511 pending baseline files; full repository Python removal remains incomplete.
Final exact-source CI and user-owned normal boot/recording/upload remain separate.

Final family gate additionally passes all25 disk/routing/workflow tests and strict
scoped Clippy. The callback/helper live-drift regressions pass again with originals
absent. All68 existing/new fast integration targets remain selected exactly once
across the three native workers; Python/Android and service checks are retained.
Published8881f354 fast PR/push, Integration and mapped-doc gates pass; Rust CI had
36 successful jobs and9 outstanding at last inspection, with no reported failure.
Those results do not substitute for this later commit's exact-source gates.

### Native diagnostic scenes and raster checker (2026-10-18)

The diagnostic source helper and checker are retired after independently porting
scene policy into Rust. A narrow owned C++ raw raylib ABI exposes drawing/resource
calls; it contains no candidate Renderer policy or Python execution. Original
shader, geometry, font, touch colors, inherited environment behavior and three
scene comparisons remain. The checker uses the full native image-array decoder
and existing NumPy-equivalent integer comparison, including shape/broadcast errors.

Six fresh original/reference PNGs are byte-identical; sixteen independently
captured environment cases match exit, image and FFmpeg arguments. The actual
current diagnostic_render candidate passes burn-in/grid/touches with zero differing
pixels. Eight integration tests, two unit tests, scoped strict Clippy, all three
tool builds and current candidate build pass. Parent repeats all eight integration
tests and actual three-scene checker after deleting the two Python files with
PATH=/nonexistent. Native exec traces contain no interpreter. Source/font/layout
and all five dependency records remain guarded; a newly importable Params binding
requires independent language/font recapture instead of silently assuming English.

PROFILE_STARTUP retains initialization-only exit, no PNG writes, initialization
failure precedence and cleanup. Its output intentionally reports actual native
initialization time instead of CPython cProfile's function counts/cumulative table;
this implementation-specific report is not a Python profile or speed comparison.
RECORD starts the original FFmpeg argument list, feeds no frames in this helper,
and closes/reaps its owned recorder; failure also reaps the raw drawing child.

The complete UI orchestrator now selects check-ui-diagnostics. Required CI builds
all three tools, runs both native suites and builds/exports the raw helper under
explicit 512MiB/16MiB growth checks. The new routing assertion first failed before
migration, then all28 affected routing/disk tests passed. The other four Python UI
children and the orchestrator itself remain pending; a Rust wrapper around those
children is not counted as their conversion. Inventory:71 replaced-local /
3,509 pending baseline sources. Evidence:2026-10-18-ui-diagnostics/{production-final,
production-final-six.json,parent-retired-proof,parent-retired-tests.log}.

Exact e5eab025 PR fast run39065270930 exposed one stale full-command assertion
in ci_rust_isolation: the UI build now includes three diagnostic tools, while
the inherited expected command still ended after ui-polygon-reference. The
assertion now requires all old tools plus the three new tools. No required
coverage or workflow execution was removed. All48 isolation, polygon routing
and disk-bootstrap tests pass locally after the correction. The same CI run's
alert, corpus and Python/Android workers passed; Integration and mapped-doc
checks also passed for e5eab025. Subsequent exact-source checks remain required.

The next exact-head run39065901972 reached a second copy of that full-command
assertion in ui_egl_ci and failed there. Both copies now include all diagnostic
tools. All49 isolation, EGL/polygon routing and disk-bootstrap tests pass together
on the final local commands (2026-10-18-ui-forms/parent-default.log). No workflow
coverage was removed; exact-head CI must be rerun after this correction.

### Fast alert worker timeout correction (2026-10-18)

On a8189aec, the fast native alert worker exhausted its seven-minute bound:
alert_callbacks completed in23.37s, alert_manager in243.36s and selfdrive_events
in20.08s; selfdrive_helpers was cancelled during its final corpus test. Split
alert_manager into its own mandatory seven-minute worker. Every prior suite
still runs exactly once, and the aggregate requires all five workers to succeed.
Missing/failure/cancelled/skipped/empty results remain failures. The new routing
and result-binding tests first failed twice against the old workflow, then all
52 CI routing/isolation/disk tests passed with the split. Evidence is in
2026-10-18-ci-diagnostic-routing/alert-split-{red,green}.log. Remote timing
confirmation remains pending; this does not establish runtime improvement.

The same exact-head Rust run39066510840 exposed a selfdrived source comparator
still importing deleted checker helpers and a planner IPC first-cycle validity
difference (three records at the second model input on x86_64; ARM passed).
Both remain under investigation; neither is treated as a passing required gate.

### Live Params checker conversion (2026-10-18)

`check-selfdrive-params` replaces the fixed Python checker using unchanged C++
Params methods, actual filesystem state and native ZMQ sockets. Its source side
also matches an independent47-case original Cython capture, including seven
fatal std::stoi diagnostics and six conversion warnings. The test peer supplies
controlled candidate failures only; it is not the production oracle. The new
`--reference` replaces `--binding`, and reports identify the native reference
instead of claiming a Cython invocation. Generic Params/logging sources remain
live and pinned. The optional selfdrive-params-native feature keeps ZMQ out of
default developer-tool builds.

Nine unit tests, five integration tests (including15 original boundary outcomes),
strict Clippy, current-source candidate differential and copied-root retirement
proof passed. Parent repeated the actual47-case comparison after deleting
check_selfdrive_params.py, with PATH=/nonexistent; stdout/stderr match original
captures byte-for-byte. Timeout/error cleanup terminates owned groups and reaps
direct children. PR_SET_DUMPABLE=0 in the QA helper prevents an external host
crash collector while retaining all seven original SIGABRT outcomes; production
Params is unchanged. Independent implementation review found no blocker.
Evidence:2026-10-18-selfdrive-params/{current-source-proof.log,
current-retired-root-proof.log,parent-retired.stdout,parent-retired.stderr},
plus2026-10-18-batched-matmul/params-review.txt. CI now builds/tests the native
checker/reference and retains all other source comparisons.

### Planner IPC comparison pacing repair (2026-10-18)

The x86_64 run39066510840 source's first model computation took13.36ms while the
native run took3.83ms. The harness added50ms after each completed computation,
producing approximately63ms versus54ms input periods. This crossed the unchanged
minimum receive-frequency boundary only for the original; records3–5 differed
in validity at the second input. Pace normal inputs from their previous send,
and retain a complete recovery interval when computation overruns an entire
period. Record actual send times. No publication or validity comparison is
removed and no production cadence/validity rule changes.

Two scheduling regressions first failed, then all five planner CI tests passed.
The exact failed-run native executable/solver artifacts were replayed with the
fixed harness against live original code: two original and two native runs each
produced121 messages, matching all compared values/validity, both triggers and
34 fast overlays. All three comparisons reported no differences. These are
host checks; latest x86_64/ARM remote checks remain required. Evidence:
2026-10-18-ci-diagnostic-routing/planner-pacing-{red,green,live}.log.

### Retained SelfdriveD source-loader correction (2026-10-18)

The fixed callback/helpers checker CLIs were ported, but SelfdriveD's live source
oracle still imported their generic loader functions. Keep those exact loaders
in selfdrived_helpers_source.py and update the caller. This is retained Python,
not native conversion: the two baseline inventory rows are now partially-replaced.
The added loader and its four regressions remain pending tooling work alongside
the controller comparator. Baseline totals after Params:70 replaced-local,
2 partially-replaced,3,508 pending. Added Python files are explicit above and
are not hidden by the baseline count.

Five original function ASTs and295 requests/9,436 controller steps match the
pre-retirement source including messages, state and effects. Four fresh loader
regressions pass and CI requires them with actual original bindings. A static
audit of deleted-tool imports found no other missing imports; this does not
prove dynamic imports are absent. Parent's final Params gate passed9 unit and
60 integration/routing cases plus strict Clippy. The initial local rerun lacked
the non-system Cap'n Proto include prefix; setting SELFDRIVE_PARAMS_CAPNP_PREFIX
to the installed SDK resolved that build configuration error. CI installs the
system headers. Evidence:2026-10-18-selfdrived-oracle-regression and
2026-10-18-selfdrive-params/parent-final-sdk.log.

Parent then built controller from the current #296 source into the isolated cache
and ran the unchanged full controller comparator against the relocated live
oracle. All295 requests/9,436 steps passed, retaining both original exceptions.
Candidate SHA256:289046d73988b02d9ae61e79e8727908b00cbfd114aa8f75a64dc3592d97f5c2;
evidence:2026-10-18-selfdrived-oracle-regression/current-controller.log.

Full legacy UI caller regression found two stale Python-command expectations
for already-native polygon/diagnostics children (normal and EGL-failure cases).
Update both expected invocations and add failure-stop cases for each child,
retaining all order/log/exit/remaining-Python checks. Original two failures then
all12 caller tests pass. Evidence:2026-10-18-ci-diagnostic-routing/
ui-routing-{red,green}.log. This changes only regression expectations.

### Separate complete Ratekeeper fast worker (2026-10-18)

Head5fbc1193 Fast checks run39068241807 timed out in native-corpus after
Ratekeeper passed all four cases in167.60s and SelfdriveD passed in48.61s;
the job then reached stats_numbers before its seven-minute bound. Routing,
alerts, alert-manager and Python/Android workers all passed. Preserve every
suite and move the complete Ratekeeper corpus into its own mandatory
native-timing worker, retaining the bound, disk guard and failure aggregation.
Two routing/admission regressions failed before the workflow/policy change;
all57 selected CI/UI routing tests passed afterward, including absent,
cancelled, skipped, failed and bypassed timing-worker mutations. Evidence:
2026-10-18-ci-diagnostic-routing/timing-worker-{red,green}.log.
The source Ratekeeper behavior and corpus are unchanged; remote exact-head
execution still determines whether the split fits the hosted-runner bound.

Head5fbc1193 Integration completed successfully in run39068241794 and mapped
docs in39068241738. Rust run39068241872 remains in progress at this checkpoint.
These results do not validate subsequent source changes or device behavior.

Follow-up exact-head7515a494 Fast checks PR run39069016854 passed all six
workers and aggregate. The split therefore preserved the full corpus within
the hosted seven-minute bound for this run. Earlier Rust39068241872 was
cancelled by the newer same-PR concurrency run, not completed successfully;
latest Rust39069016848 and Integration39069016906 remain the relevant checks.
The current device/user-owned validation decision and checkpoint are recorded
in issue296 comment6241121938; overall conversion acceptance remains open.

### Complete plot sampling helper and checker retirement (2026-10-18)

Replace check_ui_plot_samples.py and ui_application_qa/plot_samples_source.py
with an independent native sampling implementation and fixed original checker.
The generic reference retains the20Hz gate, capped clock catch-up, one sample
per frame, reset/ring/monotonic-queue order, NaN/signed-zero behavior, lazy
conversion and Python Unicode numeric/error rules. It never imports the UI
candidate's Samples implementation. The checker preserves every original5000
step and53 full-history boundaries, candidate failure/cardinality handling and
partial artifact order. Add it to mandatory UI application CI and preserve the
remaining stages and their failure stopping.

Missing-module RED then source/checker GREEN:108 independent generic cases,
5000 original snapshots,13 original process/artifact cases, source/reintroduced
helper guards and CLI/routing tests. Parent's selected63-test gate and strict
Clippy passed. Original caller missing-stage RED then14 caller tests passed.
The current-source UI example built successfully and passed5000/53 both before
and after removal of the two originals. PATH=/nonexistent execve contained
only checker and candidate; native candidate SHA256:
6769313fa97c9b231e0fe878bf71f362f05839c80a8e9bf3a00f0c784906e1d4.
Five actual child-failure cases preserve the original partial artifact set.

Independent reviewer found no blocker and checked90 additional arbitrary
inputs plus numeric/long error cases. Parent also compared120 randomized
original/native timelines with resets, ring wrap, nonfinite/signed-zero inputs
and complete histories. Evidence:2026-10-18-plot-samples/{first,final,clippy,
current-candidate,retired-candidate,after-deletion-tests,random-generic,
native-failure,caller-green}.log and2026-10-18-plot-independent-review/REVIEW.md.
These are host tool-equivalence checks, not device/driving validation. Baseline
inventory now72 replaced-local,2 partially-replaced,3,506 pending; full
repository conversion and exact-head remote acceptance remain open.

### Native fixed emoji raster gate (2026-10-18)

`check-ui-emoji` now replaces `rust/tools/check_ui_emoji.py` in mandatory UI
connectivity CI. All seven original texts and independently captured source
rasters remain exact. The generic `emoji.py` implementation stays separately
pending; this fixed checker does not claim to replace its callable renderer.
The native decoder retains Pillow array modes and content-based format selection.

Retain the existing21 original process/codec scenarios and12 new original cases,
including TGA/TIFF/losslessWebP/RGBA16, grayscale/palette broadcasting, hiddenRGB,
malformed/missing/truncated outputs, source-file rewrite, report-write errors and
late child failure. Missing-module RED was followed by two actual behavior REDs:
the draft reread a mutable source PNG and accepted changed reference dependency
versions. Compare immutable captured pixels and verify selected Pillow/NumPy
lock versions after fixing these. Independent review identified the latter gap.
Both regressions now pass without relaxing pixel equality or failure order.

Native6test suite passes, including33 original scenarios, exact artifact hashes,
source/font/dependency drift, CLI arguments and real inherited stdin/stdout/stderr.
The47 CI isolation/disk/routing tests and strictClippy pass. Current candidate
`emoji_raster` SHA256 is
99cc517371ba8a72cc71db53e7de774b0229f86cfc995bb3ccfd58edc9208152.
Actual seven-image comparison passes before and after Python checker removal;
PATH=/nonexistent execve contains only the native checker and candidate. Seven
post-deletion tests pass. Evidence:2026-10-18-emoji-check and independent review
2026-10-18-emoji-independent-review/REVIEW.md. Inventory:73 replaced-local,
2 partially-replaced,3505 pending. Full repository conversion remains open.

Prior exact-head89a6294c Fast PR39070586386, Fast push39070580714,
Integration39070586387 and mapped-docs39070586381 passed. Rust39070586378
continues; no device result or uncompleted Rust-job success is inferred.

### Native fixed registration UTF-7 gate (2026-10-18)

`check-registration-utf7` replaces the Python checker in startup CI. Preserve
all107 actual CPython UTF-7 results and22 original checker process/JSON/artifact
cases, including runner prefixes, Unicode splitlines, numeric equality and
strict child-pipe decoding. Source/corpus pins prevent silent expectation drift.
The native registration decoder is unchanged and never generates expectations.

Missing-module and CI-routing REDs preceded implementation. Independent review
reproduced a report-write boundary mismatch: at candidate nesting993 the original
CLI raises before creating a report, while the draft created one. Regression
RED then source-context serializer bound995 GREEN preserves992success/993failure
and binary digest/read ordering. Four checker tests, one routing test,53 combined
CI policy tests and strictClippy pass. The actual current registration_utf7 example
SHA2568fb0b0f90859e74e8292cd201bccf0f5c4843837cfb2556d89940026fa61ba98
passes all107 cases before and after original deletion. PATH=/nonexistent execve
contains only checker/candidate. Four post-deletion tests also pass.

The new native test step keeps an explicit512MiB build growth guard. Existing
EGL/diagnostics routing assertions initially omitted the new emoji CLI flags;
exact remote failure117510817489 and local companion failure were corrected in
f1349040 without skipping tests. Earlier Rust39070586378 was cancelled by the
newer commit while some jobs remained; its aggregate failure reflects cancelled
prerequisites and does not establish a full Rust pass. Latest exact-head CI must
complete before integration. Evidence:2026-10-18-utf7-check and independent-review.
Inventory74 replaced-local,2 partially-replaced,3504pending; full conversion,
source-bound packaging and user device acceptance remain open.

### Native Unicode digit-table generator (2026-10-18)

`generate-bluetooth-digits` replaces `generate_bluetooth_digits.py` using the
unmodified, hash-pinned Unicode15.0.0 DerivedNumericType database, with its
license. It computes the Digit/Decimal union and decimal ten-digit starts;
expectations are the unchanged original's exhaustive1,114,112-codepoint output,
not the existing runtime tables. Original stdout is byte-identical; after
Rustfmt it also exactly reproduces the runtime Bluetooth table. The original
generated provenance comment is intentionally retained in the output.

Missing-module/CLI RED preceded implementation. Both native API and actual CLI
tests pass, including ignored arguments and PATH=/nonexistent. Scoped strict
Clippy passes. After removing the original script, both tests still pass and
execve tracing shows only the Rust generator. Independent source review found
no actionable issue. The six fast routing/policy tests pass after preserving
all existing suite assignments and adding the generator test. The final eight
UTF7 original-CLI boundary receipts from the preceding unit are also retained.

Source/runtime policy and Bluetooth digit acceptance are unchanged. Baseline
inventory is now75 replaced-local,2 partially-replaced,3,503pending. This is
one developer generator, not completion of repository Python retirement.
Evidence: local2026-10-18-bluetooth-digits/{red,green,clippy,routing-red,
deleted-tests}.log and after-deletion.execve; UI-coordinated routing GREEN in
2026-10-18-ui-forms/anyvalue-repair-family-ci.log. Exact preceding head5226b36d
Fast checks39072471346 and mapped-docs39072471414 succeeded; Rust39072471343
and Integration39072471378 remain in progress at this checkpoint. Neither
device acceptance nor complete source-bound packaging is claimed.

### Native Forms checker and reference (2026-10-18)

`check-ui-forms` and `ui-forms-reference` replace both original Python tools.
The reference implements all eight widget families behind an independent raw
raylib drawing boundary; it does not call candidate widgets or replay stored
rasters as implementation. The unchanged original supplied the expected pixels,
states, drawing traces, admission failures and artifact/lifecycle boundaries.
Current live source, font, asset and dependency pins remain enforced. Only the
two retired helper/checker files may be absent under immutable capture hashes.

The final gate passed208 family cases,65 admission cases,35 checker controls,
nine reference tests, snapshot ordering, HTML codepoint parsing, six CI-policy
and four routing tests, plus strict Clippy. Concrete mismatches repaired include
lazy surrogate conversion, mutable list aliasing, constructor/setter failure
order, deep JSON export boundaries and Slider shimmer using all cached line
widths rather than only visible lines. Expectations and pixel tolerances were
not relaxed. Startup profiling deliberately reports native timings in place of
CPython cProfile tables, preserving early-exit and artifact behavior.

Actual native-only execution with PATH=/nonexistent matched all18 scenes and
388 frames exactly: zero different pixels, zero channel differences and equal
full state. The same18/388 proof passed after removing both Python tools;
execve contains only native checker/reference/raw drawing/candidate processes.
Mandatory CI builds the drawing boundary with warnings as errors, checks its
presence and runs all five Forms targets under the supplied Xvfb display before
the full UI gate. The enclosing framework driver remains explicitly Python
until its own conversion is validated. Independent review found no outstanding
Forms policy blocker.

Evidence:2026-10-18-ui-forms/closure-{package,parser,encoder,routing,clippy}-v1.log,
closure-validation-v1.json, after-deletion.log and after-deletion.execve.
The post-deletion source/dependency drift regression also passed in92.25s;
missing live sources and altered reintroduced originals remain rejected.
Inventory77 replaced-local,2 partially-replaced,3501pending. The user's latest
instruction makes physical normal-boot/record/upload verification user-owned;
this local tool validation does not establish those device results.

### Native camera source generators (2026-10-18)

`generate-camerad-requests-reference` and `generate-camerad-packet-reference`
replace the two Python generators, reading current caller-supplied C++ source
and templates. Extraction boundaries, optional packet closing semicolons,
ordered replacement of every placeholder, universal newline decoding and
CPython trailing whitespace semantics are preserved. No generated C++ output
is embedded as production behavior.

The unmodified originals supplied27 changed-source-tree outcomes and14 CLI
outcomes before implementation. Missing native module/binaries produced RED;
all native comparisons then passed. The caller routing regression reproduced
the old Python invocation before migration. All61 focused generator/CI-policy
tests and scoped strict Clippy pass. Independent review separately recaptured
all41 original outcomes and found no actionable issue. Both originals were
removed; all four generator tests still pass. PATH=/nonexistent execve contains
only the native generator, and actual CameraCheck.generate invocations produce
the original bytes. CI builds/tests both commands before the original camera
comparison suite, with two explicit1024MiB growth guards.

The generated original C++ was compiled and compared against current camera
examples:512 request scenarios/72,493 steps and3,281 packet cases/5,869,496 bytes
all passed. Local initial compilation attempts lacked generated Cap'n Proto
include paths; GCC also rejects inherited C99 array designators. The complete
include setup and Clang build passed without changing original source; the
inherited11 extension warnings remain. CI already uses Clang18. These are host
source/packet checks, not camera hardware or vehicle evidence.

Evidence:2026-10-18-camera-generators/{red,first-green-routing-red,routing-disk,
green,clippy,postdelete}.log, caller/commands, requests-check/report.json,
packets-check/report.json and postdelete-packets.execve. Inventory79
replaced-local,2 partially-replaced,3499pending; full conversion remains open.

### Camera generator closure and shared render reference (2026-10-18)

Eight lifecycle/fixture commands plus the BPS and sensor constant generators now
read live C++ inputs through Rust. Their ten Python originals and both shared
render Python tools are retired after caller migration. The inventory is91
replaced-local,2 partially-replaced,3487pending. Generic camera comparators,
network/application reference tools and the wider compiler/tool tree remain
explicit dependencies; this is not repository-wide completion.

Camera original captures cover138 changed-source outcomes,9 extraction
boundaries and67 CLI cases. All12 actual generated C++ modes match the original
bytes, including after deletion with PATH=/nonexistent and native-only execve
traces. BPS covers41 original cases and exhaustive Unicode15 word/digit/case
mapping digests. Review caught Rust's newer Unicode tables accepting names that
CPython3.12 rejects; the fixed tables preserve the original classes. Sensor
constants cover70 original cases, nested JSON, numeric/surrogate output and
failure phases. The actual three-sensor C++ reference produces45354 identical
bytes. Review additionally reproduced relative Path normalization changing
executable lookup;10 independent path cases retain that subprocess behavior.
The sensor-only runtime-version dependency is optional; default dev-tools has
no runtime-version, cereal, capnpc or messaging dependency in its Cargo tree.

The generated C++ also passed the complete host camera comparisons:535020
sensor cases,99 exposure scenarios/54108steps,512 request scenarios/72493steps,
3281packet cases/5869496bytes,334kernel,62sensor lifecycle,30ISP,32camera
lifecycle,8cleanup,1080state frames,54stress and8runtime termination/error/order
scenarios. Local sanitizer/library paths initially prevented execution; the
scratch launcher relocated only SDK paths while keeping comparator assertions
unchanged. Those initial failures remain recorded, followed by successful runs.
This does not establish hardware ioctl behavior, physical boot or driving.

The independent render reference implements labels, unified text, buttons and
Toggle behind an owned raw raylib drawing helper. Thirty ordinary,70raw and14
environment captures plus13checker controls pass; source/font/dependency drift
and owned-child cleanup are checked. Independent review reproduced six
multiple-release Toggle mismatches; applying every release in sequence fixes
them without changing expected pixels or state. Both originals are absent in
the final source guard and actual six-scene/112-frame gate; all pixels and state
match, and execve tracing with PATH=/nonexistent contains only native commands.

Evidence lives in ignored2026-10-18-camera-{data,lifecycle-generators} and
2026-10-18-ui-render directories. Retained tracked fixtures include original
hashes, inputs and expected outputs, not generated images or private logs.
Remote17c0a425 Fast checks, Integration and mapped-docs gates passed, while
Rust run39075635257 has a UI/connectivity failure at native JSON11 staging.
The wrapper hid stderr; printing it on failure is covered by a real child
regression. The last observed27.775GiB free preceded an89second build, making
reserve exhaustion plausible but not proven. UI debug symbols are disabled
without changing assertions/optimization; a4096MiB prebuild capacity guard was
added. Exact next-commit CI must confirm the repair.

The registered `check-ui-framework` orchestration also passes22 original injected
contracts,5 actual original process fixtures, source-drift rejection and strict
Clippy. Its real13-stage invocation passed every stage: widgets, scroll,
navigation, stack, scroller, render, Forms, network, polygon, diagnostics,
application, FPS and EGL. The live X11 test entered`native`, closed normally,
and transported logs; FFprobe confirmed536x240 recording with3video frames
from9render frames. Network/application remain explicit Python children, and
the existing Python orchestrator/tests remain until their caller-test migration.
This host UI recording proof is not comma-device normal boot/route recording
or network upload acceptance. Current-source evidence:
2026-10-18-ui-framework/{second-gate,actual}.log and actual/invocations.json.

### Shared UI caller retirement and CI routing repair (2026-10-18)

`check_ui_framework.py` is now removed and CI invokes `check-ui-framework`.
The seven legacy caller cases are preserved as independent original captures
alongside22 injected contracts and5 actual process cases. Network and application
remain explicit Python children. The source removal raises the inventory to92
replaced-local,2 partially-replaced and3486pending.

The full13-stage host gate passed after deletion on its own Xvfb display,
including real X11 input, normal application exit, recording, logs and EGL.
An earlier shared-display run lost keyboard focus while another worker opened
windows; isolated-display execution resolves that test interference without
changing input assertions or product code. Native routing tests pass locally.
The old EGL CI guard required an exact uninterrupted command string and failed
when valid additional binaries were appended. It now requires each original
binary argument and feature/locked flag independently; none is optional.

Published d2007fa1 Fast run39078224389 failed at that stale EGL assertion.
Integration39078224403 failed resolving Maven/Kotlin dependencies before Java
compilation; the failed jobs were retried without dependency/version changes.
The exact Rust run39078224320 is still pending. These failures are not recorded
as successes. Separate multi-release Forms behavior is tracked in
[#350](https://github.com/bin9208/openpilot-rust/issues/350).

Local evidence:2026-10-18-ui-framework/owned-after-deletion.log and
owned-after-deletion/invocations.json;2026-10-18-camera-data/routing-final.log.
Device boot, route recording and upload remain user-owned and unverified here.

### Dynamic native camera sensor comparison (2026-10-18)

`check-camerad-sensors` replaces the sensor CLI, dynamically reading current C++
configurations and generating seeded requests in Rust. The actual535020-case
run with PATH=/nonexistent matched every original result and produced a
byte-identical report, including input digest
`067eeb0afb1d9f4654a49957d007977dfb298816a4163ee9e76f71f377faf568`.
The current C++ source and Rust sensor example hashes are in that report.
The post-trim run also passes. This is host sensor-policy evidence, not hardware
camera operation or device startup.

Eight native tests preserve all268 live request-batch digests, changed configs,
CPython random-bit consumption/number formatting, structural/f32 comparison,
raw runner bytes/path normalization, process failures and distinct JSON/parser/
comparison/receipt depth boundaries. Another50 generator/routing/disk checks
and scoped strict Clippy pass. The migration test first rejected the old Python
caller; CI now runs/tests the native checker and fingerprints its binary/source.
Four remaining Python caller tests and Ruff pass.

The old sensor module still supplies unchanged `f32`/`equal` to exposure, misc
and state comparisons; AST equality and all three imports were verified after
removing its unused CLI/process implementation. It is therefore partially
replaced, not deleted:92sources replaced,3partial,3485pending. Independent
captures and8tests live in `dev-tools/tests/fixtures/camera-sensor-check` and
`tests/camera_sensor_check.rs`; ignored live proof is in
`2026-10-18-camera-sensor-check/{live-native,native-proof.log}`.

The unchanged d2007fa1 Integration39078224403 retry completed successfully,
including Android protocol/Java inference. The initial Maven resolution failure
remains recorded above; no dependency or test change was needed for the retry.

### Repeated UI release callbacks (#350,2026-10-18)

Independent original captures exposed10 mismatches in12 reference scenes and
five in the production Confirm dialog. Boolean click aggregation lost second/
third keyboard/list releases; production Confirm stored only the last pending
result. The reference now handles every successful release and production
queues results in order, retaining deferred navigation callbacks. MultiOption
uses the same queue boundary.

All12 original/reference/current-candidate pixel/state comparisons pass after
repair. The18-scene/388-frame Forms gate has zero differing pixels and equal
states;55 execve entries contain native programs only. Twenty affected Forms
checks, the production2/3-release callback-order test and strict Clippy pass.
The original failing captures are retained as regression fixtures. This is
synthetic host UI input evidence, not physical touchscreen/vehicle acceptance.

Exact d2007fa1 Rust CI still fails the one-frame `irrelevant-input-prop` Confirm
PNG hash (observed48ced25f..., expected35a7f1f0...), despite identical state.
Both GCC and Clang helpers locally produce the original expected hash. Root
cause remains open: no expected hash or image tolerance was changed. Failure-only
retention now saves the actual PNG/state/scene, stdout/stderr, expected capture,
GL environment and executable/library hashes under the existing CI artifact
root. Its controlled mismatch test and strict Clippy pass; successful comparisons
create no extra image artifact. The next exact-source CI must provide the missing
render evidence if the mismatch repeats. See #350 and local
`2026-10-18-ui-network/{releases-green,releases-eighteen,diagnostic-unit.log}`.

### Native request comparator retirement (2026-10-18)

`check-camerad-requests` now owns the dynamic request test generation and comparison.
Independent original captures cover all 512 scenarios, 43 batches and 72,493 steps,
18 process/artifact failure cases, CLI admission, 28 recursion boundaries and raw
Unix path behavior. Five focused tests and four generator/routing tests pass;
strict scoped Clippy passes. Original subprocess timeout and ordered failure
artifacts remain unchanged. Independent review found an added capture-size limit:
a 16 MiB + 1-byte stderr probe passed Python and failed native before correction.
Both streams now retain the original unlimited capture policy; the same actual
probe passes both implementations and tests assert the stream limits.

The original Python entry point has no importing consumers and was removed after
CI/caller migration. The post-deletion actual CLI run with PATH=/nonexistent passes
all 512 scenarios/72,493 steps against the current C++ source executable, and its
report is byte-identical to the original. This is host comparison evidence, not
camera hardware acceptance. Inventory: 93 fully replaced, three partial and 3,484
pending. Exposure/misc, network UI and the model compiler remain separate pending
work; no complete repository retirement or measured CPU improvement is claimed.

Verified earlier commits through `4b5eef7a13d1d86c48b339138879902fd85b458f` were
pushed to PR #343 targeting dev. New exact-source checks are tracked in
[issue #296](https://github.com/bin9208/openpilot-rust/issues/296#issuecomment-6243349766).
The device remains user-owned for pull/boot/recording/upload verification.

### Native exposure and camera utility comparators (2026-10-18)

`check-camerad-exposure` and `check-camerad-misc` dynamically generate the original
seeded inputs and compare current source/native executables. Eleven focused tests
pass, covering every request and all 183 generated images, process/CLI failures,
lazy per-row JSON parsing, deep comparison/serialization boundaries and raw Unix
paths. Strict scoped Clippy passes. Initial misc process-test failure was due to
absolute temporary paths inside the hashed payload; the corrected harness proves
both normalized inputs and restored original-context raw hashes before comparing
receipts. Expected hashes still come from original execution, not the replacement.

Actual post-removal execution with PATH=/nonexistent passes 99 exposure scenarios,
54,108 steps/54,207 observations and all 7,292 camera utility cases; both complete
reports are byte-identical to the independent original reports. The CI dispatcher
and source/binary receipts now select these native tools. All 20 dispatches and
four retained artifact-path regressions pass. The exposure Python file retains
only the exact unchanged `quoted` helper imported by the remaining state checker;
its AST was checked against the original. The misc Python entry point is removed.
Inventory is 94 replaced, four partial and 3,482 pending. Camera hardware, actual
device boot/recording/upload and full repository conversion remain unverified.

### Native camera packet comparator (2026-10-18)

`check-camerad-packets` replaces the Python packet comparator, preserving dynamic
seeded generation, 50-case batches, both-child-before-artifact ordering, lane
exit handling, JSON/depth failure receipts and original byte-count semantics.
Five focused tests pass against independent original captures: all 3,281 cases,
66 input batches, 35 process/error cases, CLI admission, recursive JSON and raw
Unix paths. Strict scoped Clippy passes. After removal of the Python entry point,
the PATH-disabled actual comparison passes all 5,869,496 bytes; its full report
and inputs.jsonl are byte-identical to the original. All five policy checks now
use native commands; the 20-command camera dispatcher remains intact. Inventory:
95 replaced, four partial, 3,481 pending. Lifecycle/state/runtime/CI orchestrator
families remain explicit pending work; this is not a whole-repository milestone.

The next kernel-comparison port needs the original child-only core-dump policy.
The native command spec now supports an opt-in child core limit through the small
`openpilot-process-limits` syscall boundary, preserving dev-tools' unsafe-code
forbid policy. The new regression first failed for the absent option, then passed
with all 25 process-runner tests: child soft/hard limits are zero, parent limits
and later unrelated child inheritance are unchanged. Strict Clippy passes. No
existing command opts in implicitly and no device scheduling policy changes.

### Native kernel comparator and child input preservation (2026-10-18)

`check-camerad-kernel` retains all 334 original scenarios, 25 baseline choices,
ordered sanitizer/QEMU environments, normalization, failure artifacts and exact
comparison rules. Four focused original-capture tests and four camera routing
checks pass, as does scoped strict Clippy. Review found an inherited-stdin
regression: an actual controlled original child read `token`, while the first
native implementation read EOF. The assertion failed before the fix; the native
command now inherits stdin, and the same original/native probe reads `token` in
both implementations. Core limits remain confined to the spawned child.

All 334 actual source/native kernel comparisons pass with PATH=/nonexistent and
the complete report is byte-identical to the independently captured original.
This host has LLVM18 in the existing local SDK rather than the original fixed
`/usr/lib/llvm-18` path. A mount namespace exposes that SDK at the original path,
with its own `/dev` and `/proc`; neither comparator source nor sanitizer settings
were patched for the native proof. The initial missing-library failure remains
recorded separately and is not counted as a comparison pass.

The camera CI dispatcher now selects and hashes the native kernel command;
all 20 dispatches and four retained artifact-path tests pass. The Python file
retains only `digest` and `disable_core`, whose ASTs are unchanged, for the three
remaining lifecycle callers. Inventory: 95 fully replaced, five partial and
3,480 pending. This is host verification; the user will perform physical boot,
recording and upload verification after code delivery.

### Temporary CI-only Confirm raster investigation (2026-10-18)

The exact `0b6c4e02` CI failure retained PNG/state, rendering and source receipts.
Native output differs from the local independent original by 171 pixels; PNG
encoding alone is ruled out. The raylib library hash and reported Mesa/LLVM
versions match locally. CPU capability probes change raster output but have not
reproduced the exact CI hash, so CPU/JIT causality remains unproved.

A temporary failure-only diagnostic restores the hash-verified original Forms
oracle from immutable commit `3669ec8c6f391275316db31bfd715ab23f0da5da` into runner
scratch and runs it alongside the native reference on the same display. It
captures exact images, state and CPU/GL metadata, without replacing or relaxing
the failing test. Locally both independent lanes match the original expected hash
and have zero differing pixels. This temporary Python diagnostic is not a native
replacement or permanent source fallback. Remove it after the failure is repaired;
repository-wide Python retirement remains incomplete while it exists.

### Native sensor lifecycle comparator (2026-10-18)

`check-camerad-sensor-lifecycle` preserves all 62 scenarios and the complete live
source/native trace comparison. Four focused tests pass against 37 independently
captured process/failure cases, six CLI classes and real inherited-input/raw-path
execution. The portable-artifact assertion formerly in the Python test now checks
every native report row and both lane receipts. Trace JSON still parses before
run.json is written, and CLOEXEC differences remain failures rather than receiving
the kernel checker's normalization. Scoped strict Clippy and all four camera
routing checks pass; the routing assertion first failed before migration.

The actual PATH-disabled comparator passes all 62 cases after Python removal,
with its complete report byte-identical to the independent original. The same
private LLVM18 SDK mount used for the kernel proof preserves sanitizer settings.
All 20 dispatcher entries remain; the three unrelated camera artifact regressions
still pass. Python digest/core helpers remain for ISP/camera lifecycle callers.
Inventory is 96 replaced, five partial and 3,479 pending. The remaining embedded
CI diagnostic and full model/development/test tooling are not counted as retired.

### Native ISP ownership comparator (2026-10-18)

`check-camerad-isp-lifecycle` preserves all 30 full/two smoke cases and the original
resource ownership comparison: source-leaked descriptor closes, BPS configuration
unmaps, immutable release bytes, first-failure stopping and narrow assertion
handling. Five tests pass against 45 original process captures, 33 normalization
cases, six CLI captures and real inherited-input execution; scoped Clippy passes.
The initial test diagnostic required removing unsupported Debug formatting for
JsonValue, without changing the test's expected results.

The current native comparator passes all 30 actual source/native cases with PATH
disabled and the exact full report matches the independent original. The Python
CLI is removed while the `normalize` helper remains AST-identical for camera
lifecycle. Its generic invalid-input and ownership behavior remains part of the
Rust tests. Inventory: 96 replaced, six partial, 3,478 pending. These host results
do not establish hardware operation or whole-repository conversion completion.
