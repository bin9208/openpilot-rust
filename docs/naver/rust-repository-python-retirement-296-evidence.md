# Repository-wide Python retirement: current evidence (#296)

This is an unfinished conversion, not a completion report. Base commit:
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Work is isolated on
`codex/feat-296-repository-python-retirement`; concurrent `dev-md` work is not
part of this candidate. No device validation or release is claimed here.

## Inventory

The [source inventory](rust-repository-python-retirement-296-inventory.tsv)
records all 3,580 baseline Python/Cython/SCons source paths and their original
Git blob identities. It includes 3,542 `.py` files, eight Cython files and 30
SCons entrypoints. Thirty-two sources have locally verified replacements and migrated
callers; 3,548 entries remain pending. `replaced-local` does not mean merged,
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

Continue the [implementation plan](rust-repository-python-retirement-296-plan.md)
through every remaining source family, actual consumer migration, a zero-Python
execution audit, independent review, exact-commit PR and merged-dev checks,
source-bound packaging and applicable device gates. Current local results are
not Actions evidence and do not establish repository-wide completion or CPU
savings. Issues #296 and #1 remain open.

Docs-Not-Needed: internal implementation and validation records; no setting or
user-guide behavior change.
