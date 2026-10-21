# Repository-wide Python retirement: current evidence (#296)

This is an unfinished conversion, not a completion report. Base commit:
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Work is isolated on
`codex/feat-296-repository-python-retirement`; concurrent `dev-md` work is not
part of this candidate. No device validation or release is claimed here.

## Inventory

The [source inventory](rust-repository-python-retirement-296-inventory.tsv)
records all 3,580 baseline Python/Cython/SCons source paths and their original
Git blob identities. It includes 3,542 `.py` files, eight Cython files and 30
SCons entrypoints. The current source inventory has 230 replaced-local entries,
three partial entries and 3,347 pending entries. `replaced-local` does not mean merged,
published, or that its whole parent component is converted. Embedded and
extensionless Python execution still needs a separate complete caller audit.

### Current accepted source checkpoint (2026-10-21)

Updater three-source deletion at083f937a passed
[39547256419](https://github.com/bin9208/openpilot-rust/actions/runs/39547256419):
15native tests retaining16original cases, both1,089exec roots with no Python,
strict Clippy and default CLI. Artifact12322531729 ZIP SHA256
`c0f5ef8364da8907ae059f36201d45cd9ca31aea222d34b965eee88bd9bf2c6b`;
parent compared all20native JSON and before/after seals. Source policy39547261512
passed13disk/74isolation/2lagd/2timed. Full startup-services and latest full PR remain pending.

#448 sourcea6c3201f full encoder39546722792 passed5codec/112packet hashes,
32V4L/10runtime, complete fixed-clock wire controls and Clippy. Artifact12322940409
ZIP `ed9838b0bbc902588128041ca445d3eb908451074921d442d90576fb0a2d1921`.
Selected payload-copy accounting205400→102700bytes, allocations240→240; elapsed
samples do not establish speedup. ARM/device acceptance remains pending.

Desire sourcea20ad0fa passed the fresh original/native24585-frame raw pair,
83library and4desire integration tests before4Panda fixture tests failed on optional
TOMLsize indexing.1de0b0d7 fixes that panic and adds actual same-ELF desire
private-absence proof. Both originals remain until cloud evidence and raw CLI
admissions are accepted. Native Panda full38phase pair39548089041 and desire
absence/CLI39548303946 are running. This checkpoint is not whole-project completion.

### Historical cloud checkpoint and source batch (2026-10-21)

Historical PR #343 checkpoint: `2d8d027932dc880bd0acd00288b256ac8d7f1b89`.
The published source is now`a9933ed72fc1ca8c91cf5728d0345f1e4181104d`;
newer results and pending source repairs are recorded below.
Its [Rust run39494388960](https://github.com/bin9208/openpilot-rust/actions/runs/39494388960)
passed LLVM20 graph/primitive equivalence, including all four independent
node280 input/planning controls. Workspace and ARM jobs remain running; Panda,
audio and UI/connectivity failed. Fast and Integration passed. There is no
aggregate CI success, dev integration, final package or device acceptance.
The actual lagd #429 and messaging #431 allocation evidence is recorded in
[lagd](rust-lagd-masked-allocation-429.md) and
[messaging](rust-messaging-update-allocation-296.md); neither establishes CPU
or device savings.

The focused [run39503456244](https://github.com/bin9208/openpilot-rust/actions/runs/39503456244)
explicitly checks out source `a0690dd94924996f2bc1c7da7a5dde964bf7fce3`;
its workflow head is `df267a2880ba655380a96474b62157e5434e80de`.
The actual native Eigen action and complete690-file original header comparison
passed for both x86_64 and aarch64 archive inputs on x86_64 hosts. This is not
ARM execution. The parent downloaded comparison artifacts12302561921 and
12302462410 (ZIP SHA-256 respectively
`861615c0ec94be631077444a10b7c1b153227b96a3cd703ede7de8e879a36e3e`
and `2007b0dc66fd820dae7c7d442f8b8d1bc7c847defd3eda1a90e1a7bbdf7a6d51`).
The seven native paramsd atomic-gate foundation tests passed, alongside the
five retained original Python cases. Both native estimators then passed their
original comparisons. The job subsequently failed in lagd's CI-policy test,
which still required the retired Eigen pip/import lines. The x86 action job
also failed after its successful header comparison in an audio command-text
policy assertion. Both stale contracts are corrected in the next source.
Python paramsd controllers and their descriptor-passing consumers remain.

All three embedded Eigen pip/import callers are replaced by native archive
staging and source-bound include exports. Earlier source4991f8f5 passed the
three-wheel full-header comparison and strict Clippy in
[run39500380365](https://github.com/bin9208/openpilot-rust/actions/runs/39500380365).
The parent independently verified all3 raw wheels and complete original maps
in artifact12300415646 (ZIP SHA-256
`877b1fd589b4bef46905dbad1e7d26876ec65402af698d863db28d99fa62ecaf`).
Only the C++ install tree is extracted; no Python executes or appears in that
output. External Eigen stays C++. No baseline-source inventory entry is added
for removing these embedded calls.

Audio's old-source finalization regression now produced the exact intended
exit101 and source-mutation assertion. The subsequent shell receipt check
failed because `rg` was absent; current-source tests were skipped. The receipt
checks now use `grep -Fq`. The repaired GREEN, blocked-stdout observation,
complete audio suite and Clippy remain required. See [#433](rust-audio-qa-finalization-433.md)
and [#426](rust-audio-final-stdout-observer-426.md). Panda's diagnostic
run39501353390 captured all10 timestamps in8 unchanged and8 instrumented runs;
it does not reproduce the missing first timestamp or close #432.

The actual post-deletion Bridge gate passed at source0d1700e5 in
[run39499806776](https://github.com/bin9208/openpilot-rust/actions/runs/39499806776).
The parent independently reviewed its new traffic reports, provenance, seals
and17-exec trace in artifact12299956453; see
[Bridge evidence](rust-bridge-controller-retirement-296.md).

BlueZ policy and Bluetooth enumeration now carry sealed original-only cloud
observations and source-absence preparation. Mandatory cloud lanes reuse the
same checker/candidate ELFs, compare the full corpus and require only native
execs with unavailable PATH. Their original Python files stay until those new
lanes pass. Sudo's native fixture already matched10 original modes; its two
callers now install an explicitly hash-bound native ELF, with cloud caller
verification still pending. None is newly counted retired.

The [prefix93 diagnostic](rust-prefix93-fma-diagnostic-296.md) localizes the
original LLVM18 discrepancy to contraction-dependent intermediate rounding in
the Gelu polynomial. Parent artifact and historical-object review is complete;
the native fusion fence remains and generic closure repair is unfinished.
Disposable original-capture Python/workflows are not merged as replacements.
All local work in this batch stays at source, formatting, YAML/shell syntax,
diff and artifact-data review; functional execution remains in Actions.

### Native DBC generation and package closure (2026-10-19)

Seventeen original Python generator, packaging and packaging-test files have
been removed. `dbc-generator`, `stage-control-dbcs`, `prepare-card-assets` and
`prepare-radarcan-assets` generate actual outputs from native policies and retained
DBC inputs; they neither execute Python nor return a stored asset catalog.
Independent originals establish 45 generated files/992,535 bytes, Card97 files/
4,830,112 bytes and RadarCAN103 files/5,010,641 bytes. Card's checked-in corner-radar
bytes and RadarCAN's regenerated bytes remain deliberately distinct.

Review reproduced FIFO blocking and a Python Path component-order mismatch,
including raw Unix filenames. Actual failing regressions preceded nonblocking
file admission, descriptor metadata checks and codepoint/surrogateescape component
sorting. Literal directory handling separately fixes destructive glob expansion
tracked in #375. Existing source isolation, atomic Card publication and RadarCAN
partial-failure effects remain covered. Historical removed-source hashes and
23 compiled native implementation pins are distinct from111 Card/127 RadarCAN
live input pins. Retained source changes and dangling links are rejected.

All43 DBC tests, Version foundation8, Linux process-check6 and54 CI-policy/disk/
lagd tests pass together after deletion (111 total), as does strict Clippy. The
additional Chrysler control first reproduced acceptance of a4,301-digit address
where the original raises ValueError with only partial DT output. Six independent
original controls retain4,300-digit acceptance and digit counting across leading
zeros, signs, underscores and Unicode. The native repair preserves their exact
output hashes. The Linux-only pidfd module is explicitly platform-gated.
Actual frozen
post-deletion generator/package proofs reproduce all original bytes with an
unavailable PATH and only one executable in each execve trace. The lazy facade,
three real explicit-tool SCons runs, Jot materialization callback and all eight
Card fixture stages passed. Real default SCons bootstrap also passed without a
native-tool override: generation, newly discovered dependencies, then a no-op.
The first private bootstrap proof incorrectly expected the second run to be a
no-op; its failure is retained separately, not described as a runtime defect.

The retained Python facade, SCons engine, Card comparison controllers and Jot C++
build are not retired by these results. The mandatory macOS ARM64 job retains disk reserves and the active SDK. Linux
retains all17 generator/API CLI tests; macOS has15 applicable tests, with Unicode
paths and both invalid-content decoding cases. [APFS requires UTF-8 filenames](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html),
so the two raw-filename fixture tests remain Linux-specific. Actual Darwin validation
and exact final commit CI remain required. Receipts are under
`.analysis/scratch/2026-10-18-dbc-generator/`: `complete-gate6/`,
`packages-native-postdelete6/`, `native-trace-postdelete6/`,
`callers-native-postdelete6/`, `card-stage-postdelete5.log`,
`scons-bootstrap-postdelete5/` and `chrysler-digit-limit-red.log`.
The first new-CI run failed because the exact required-job binding registry still
lacked the Darwin job; the added binding and removal mutation preserve mandatory
aggregation. That failed run remains in `complete-gate4/`. Final staged whitespace validation
required spelling one original DBC trailing space as a format argument; the
complete111-test/Clippy gate and frozen byte/package/caller proofs were repeated
after that representation-only change. The prior default-bootstrap/Card-stage
proofs remain explicitly identified by their `postdelete5` paths.

The first actual macOS job at6efa00a3 passed disk reclamation and dependency
setup, then failed to compile the Linux snapshot checker: rustix intentionally
does not expose `mkfifoat` on Apple targets. That checker also requires Linux
`/proc` descriptor/process inspection. Its module, child/checker entrypoints and
integration tests are now explicitly Linux-only; other hosts get an unsupported
platform error from those entrypoints. Review additionally found that the independent
reference imported neutral source-pin/root helpers from the checker. Those helpers
now live in a shared module, preserving their policy and the reference's portability.
Fresh Linux snapshot/launcher builds and all seven snapshot
tests pass, including the complete filesystem/child/detached matrix. Evidence:
`.analysis/scratch/2026-10-19-dbc-darwin/bootlog-{build,test}.log` and
the [failed macOS job](https://github.com/bin9208/openpilot-rust/actions/runs/39148174411/job/117736576999).
All seven tests also pass after the shared-helper move (`bootlog-shared-test.log`).
The repaired macOS job still requires exact-commit verification; tracked in #378.
The same6efa00a3 run also found the retained Card CI test's exact aggregate list
omitted the new mandatory `dbc-darwin` job. The list now includes that job without
dropping any existing requirement. All39 retained Card CI/phase-fence/admission
tests pass (`card-policy-green.log`); these Python tests remain pending retirement.

At8d47fd30, Darwin compiled and passed nine generator API tests, but the directory
cleanup case exposed a second portability defect: Darwin `unlink(directory)`
returns EPERM, whereas the Linux capture contains EISDIR. The native mapper had
omitted EPERM's Python `PermissionError` subclass. A real failing errno regression
preceded the minimal mapping repair; raw errno and raw path bytes remain intact.
Only that macOS expectation differs, following [Apple unlink documentation](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/unlink.2.html)
and [CPython's error mapping](https://github.com/python/cpython/blob/v3.12.15/Objects/exceptions.c#L3532-L3536).
The original Linux fixture and all filesystem-state assertions remain unchanged.
The new regression,43 DBC integration tests and strict Clippy pass on Linux.
Fresh native generation/package traces again match45/97/103 original assets with
one exec each and Python unavailable on PATH. Those local receipts include the
concurrent uncommitted Version Cargo registrations and do not establish an exact
committed build. Evidence: `.analysis/scratch/2026-10-19-dbc-darwin/`:
`errno-unit-{red,green}.log`, `dbc-green.log`, `dbc-clippy.log`, `generator-proof/`
and `packages-proof/`. Actual Darwin success remains pending.

### Independent Version source foundation (2026-10-19)

Commit65bdce63 ports the original Version/Git/cache/filesystem policies independently
of the runtime candidate. Eight tests preserve171 original requests,13 protocol
cases, source mutation rejection and path/descriptor controls. Independent failure
captures exposed empty cwd, numeric descriptor admission and initial cwd rename
lifetime differences; each was reproduced before repair. Owned directory identity
survives rename and caller descriptor closure without process-global cwd changes.
Eight tests and strict Clippy passed; the same eight tests passed in the combined
post-deletion DBC validation. Evidence is indexed by
`.analysis/scratch/2026-10-19-version-tools/admission-final-handoff.json`.

That foundation alone retired no Python file. The source-side policy does not use
candidate values or test fixtures as an expected-answer implementation.

### Native Version controllers and collector retirement (2026-10-19)

`check-version-reference` now constructs all17 real Git/filesystem scenarios and
compares171 requests against independent source policy, including real ZMQ error
logs. `check-version-collector` uses an isolated native worker for actual Msgq/Cereal
queues, both valid error services and exactly one disk record. The shared parent
runner retains process-group ownership through cleanup. New acknowledged tests
verify one absolute request deadline across Git calls, cancellation of live Git
descendants, streamed raw stderr, and SIGINT delivered only to the checker leader.

Independent review reproduced three controller defects before repair: doubled
spacing in `source.json`, strict UTF-8 decoding where original stdin uses
surrogateescape, and raw filesystem report strings converted into objects. The
repairs preserve original bytes, error classes and JSON string types. Both stdout
and saved reports use the same lossless formatter. The failed captures remain in
`collector-source-format-red.json` and `controller-final-review/`; original fixtures
are unchanged. A separate direct raw-basename original run timed out waiting for a
log at104 comparisons and was not used as a passing baseline or a native defect.

After deletion,96 tests pass together:12 source,11 reference/controller,4 collector
boundary,30 process-runner and39 CI-policy tests. Strict Clippy and workspace
formatting pass. Fresh unchanged original comparisons and frozen native commands
use the same current candidate ELFs and both pass171/reference and2services/1disk
collector checks. `source.json` is byte-identical. Native exec traces contain611
reference and5 collector launches, with zero Python executions; reference PATH
contains only Git and collector PATH is unavailable. Receipts are under
`.analysis/scratch/2026-10-19-version-tools/`: `controller-postdelete4.log`,
`controller-clippy4.log`, `current-original-{reference,collector}4/`,
`native-{reference,collector}-postdelete4/` and `postdelete4-receipt.json`.

Only `check_version_collector.py` is removed. Its literal path alone may be absent
from source admission; the compiled historical hash remains fixed. Mutated files,
directories, dangling links and absent retained sources fail. The native source-pin
absence test failed before this explicit transition and passes afterward. Mandatory
telemetry CI builds current tools before tests/live comparisons and rejects the old
Python command paths. `check_version_reference.py` remains byte-for-byte unchanged
for its shared `load_source()` consumers in tombstoned and managed-entry QA; its
inventory row is partial. Inventory179local/4partial/3397pending. Full conversion,
exact-commit required CI, dev integration and source-matched release remain open;
the user owns boot/recording/upload acceptance on the unavailable device.

### Native compiler node278 follow-up (2026-10-19)

The next driving prefix supports an unheld matrix/bias/ordered-SiLU result as
the packed-half input of its sole terminal matrix reader. It preserves the
physical half store and original arithmetic order, and keeps held, extra-reader
and mismatched-expression rejection. Logical channel width and physical row
capacity are distinct. No model selection or runtime validity policy changes.

Both pinned LLVM18 and LLVM20 providers passed all 18 graph targets/82 tests
and all ten primitive targets/16 tests. Independent original captures retain
97 actual compilations/786 calls per prefix, 39 generic graph contexts, physical
buffer ownership and exact output bytes. An initial fixture reconstruction
failure exposed nonzero original allocation padding; the correction retained
those captured bytes without changing defined lanes or expected output hashes.
The original failed attempt remains recorded as failed.

All-target model Clippy, scoped formatting and 54 current CI-policy/disk/lagd
tests passed. A freshly built ARM helper under QEMU matched the original A630
Add source and 2,144-byte binary; that bounded compiler case establishes neither
GPU execution nor full-model equivalence. Before/after checks bound all 225
model files and both model identities; only README evidence was amended after
completion. Parent review found no further blocker in the three production files.

Exact receipts: `.analysis/scratch/2026-10-18-batched-matmul/` contains
`NODE278-FINAL-PROOF.json`, `NODE278-COMMIT-FILES.json`,
`silu278-graph-gates-GNLNTN6B/proof{18,20}.json` and
`silu278-final-checks-BigWcXOs/`. The checked-in compiler fixture README records
input/output identities and limits. CI requires the new
`compiler_batched_silu_chain` target and retains the preceding 17 graph targets;
the workspace budget increases from 120 to 150 minutes based on the preceding
101-minute full run plus the measured new roughly 15-minute test.
Node279+, full driving/DM export and Python compiler retirement remain open.
Exact committed-head CI and user device acceptance remain required.

### Native service catalog checker (2026-10-18)

`check-services` replaces the checker entrypoint with a native comparison against
independently captured original service definitions. The candidate remains
`services_generated.rs`; it is never used to construct the expected catalog.
The checker compares all 88 definitions in insertion order and reflects all 154
fields of the full cereal Event schema, preserving the three original explicit
missing-service exceptions. Original CRLF input normalization, `--emit-rust`,
failure ordering, float formatting and generated bytes remain covered.

The original capture contains 29 CLI/filesystem cases, seven schema/phase
controls and ten numerical formatting cases. Review found a real diagnostic
difference for explicit flag values containing quotes or control characters:
12 of 18 additional original CLI cases failed before repair. Reusing the
existing native Python-compatible text representation fixes those cases,
including Unix argument bytes, without a new dependency or Unicode table.
The only shared formatter change is crate-local visibility. All six earlier
capture files remain unchanged; the additional evidence has separate pins.

Seven focused tests and strict Clippy pass. Existing service-header and
messaging/schema tests also pass. The frozen executable succeeds with an empty
PATH, emits byte-identical Rust definitions, and its execution trace contains
only the checker itself. CI now builds, tests and lints this optional native
feature before the existing message-state stage, with a fresh 512 MiB growth
guard before each Cargo command. The CI ordering regression first failed on
the Python caller; the updated workflow passes 35 isolation, 13 disk and one
message-state policy tests. Existing guard argument hashes remain unchanged;
the three additional guards bring the workflow total to 229.

Evidence is under `.analysis/scratch/2026-10-18-services-check/`, including
`missing-api-red.log`, `cli-repr-red.json`, `native-tests.log`,
`native-clippy.log`, `ci-red.log`, `ci-green.log`, `frozen-inputs.sha256`
and `native-exec.trace`. Shared `cereal/services.py` and its remaining consumers
are not retired by this checker conversion. After deleting only
`rust/tools/check_services.py`, all seven native and 49 CI policy tests pass,
as does strict Clippy across the checker and affected tests. The frozen binary
still produces the same output and exact generated bytes with an empty PATH;
its post-deletion trace contains only its own executable. See
`post-delete-tests.log`, `post-delete-clippy.log`, `post-delete-exec.trace`
and `final-review.md`. Independent review found no remaining blocker within
this checker scope. Exact-commit remote validation remains required.

### Native boot artifact checker (2026-10-18)

`check-bootlog-artifacts` replaces `rust/tools/check_bootlog.py`. It runs the
separately built unchanged C++ bootlog and current Rust executable, decodes zstd
and the full cereal schema, validates timestamps/counters/current-log identity,
and independently implements the original uploader filesystem selection. The
shared Python uploader checker remains required by other consumers.

Nine native tests retain20 scenario definitions,21 real filesystem/xattr
selections, two complete wire projections, five child/failure boundaries,
nine CLI admissions,13 shell exit cases, four raw command-output cases and all
nine required source pins. The native helper is an immutable executable; it
does not create executable scripts while concurrent tests can fork. Its exact
shell exit syntax still uses the system shell, with no Python process.

Independent review found a shared-input false positive: writing a journal tail
without a newline and immediately replacing the process lost buffered bytes.
Both comparison sides received that shortened input, so the initial20-case
pair comparison passed. Comparing complete results to the separately captured
unchanged checker exposed the missing `last` bytes. Four real raw-output
controls reproduced the loss; explicit flush before exec repairs it without
changing expected values. Generic signed/large/invalid exit syntax also retains
the original shell status and diagnostics after13-case comparison.

After removing the Python checker, the parent reran all20 actual artifact
cases, including four output faults, under an empty tool PATH. All40 normalized
result objects (both implementations) exactly match the unchanged checker
baseline. The196 exec calls contain no Python/PyPy; all nine regression tests
pass after deletion. Strict scoped Clippy passed. The existing47 CI isolation
and disk-bootstrap tests passed; the hardware job now builds/tests/lints/runs
this native checker with its complete comparison. The225 disk guards remain.

Evidence: `2026-10-18-bootlog-artifacts/{repaired-proof-summary.json,
frozen-repaired/receipt.json,parent-cross-oracle-red.txt,post-delete-repaired.log,
post-delete-repaired.execve,post-delete-repaired-tests.log}`. This is host
evidence, not physical boot/recording/upload acceptance.

The preceding exact-SHA `da803825` Fast/Integration/docs checks passed, but
[Rust run39103294926](https://github.com/bin9208/openpilot-rust/actions/runs/39103294926)
has a UI original-replay X11 connection failure tracked in
[#354](https://github.com/bin9208/openpilot-rust/issues/354). Other jobs were
still running when recorded. No complete CI, merge or release is claimed.

### Native bootlog snapshot tools (2026-10-18)

`check-bootlog-snapshot`, `bootlog-snapshot-reference`, and the native metadata
child replace `check_bootlog_snapshot.py` and `bootlog_snapshot_source.py`.
The source adapter uses unchanged C++ Params and independently implements the
original copy/thread behavior; it never imports the candidate snapshot code.
Seven tests preserve ten captured filesystem/process cases, six pipe/error
boundaries, exclusive output creation, descendant cleanup, original argument
encoding, and a real ambient-environment regression. Strict Clippy passed.

Fresh baseline and inherited `BOOTLOG_SPECIAL_COPY=1` comparisons each passed
all ten cases against the current candidate. Each trace recorded 61 exec
attempts and zero Python/PyPy executions with an empty tool PATH. All 40
invocations retain string argument arrays; separate receipts identify six
executables. After deleting both Python tools, the parent independently reran
the complete ten-case comparison successfully, again without Python execution.
The helper's process owner now observes an exited leader without reaping it
until descendant cleanup; UTF-8 errors and existing-output rejection retain
the original boundary. The original Params binding remains for other tools.

CI builds the current candidate and launcher before workspace regression tests,
and the hardware job runs the new checker, tests, strict Clippy and independent
reference builder. This is host evidence; physical boot, recording and upload
remain user-owned. The larger repository conversion is unfinished.

### Native journald validation family (2026-10-18)

`check-journald` and `journalctl-fixture` replace the Python checker, peer,
fixture input, journalctl fixture and fixture tests. Independent policy and
source-loop implementations retain real cereal/msgq/ZMQ traffic, raw packets,
log-site/source-commit checks, disk logs, fatal inputs, child signals and cleanup.
They do not call the candidate journald parser or use captured output rows as
runtime behavior. The shared logmessaged reference remains for other callers.

Fresh unchanged original captures bind 155 policy outputs, 12 child protocol
cases, nine permissive-base64 cases and the original readiness boundaries.
The current native checker and journald daemon each passed 155 messages,
13 recoverable errors and disk records, 19 fatal inputs and four lifecycle cases.
All 155 message rows matched the fresh unchanged Python original exactly.
A frozen run with only Git available in PATH recorded 147 exec attempts and
zero Python executions; no owned processes, sockets or shared-memory namespaces
remained. Three executable identities and current source hashes were verified.

Review caught and repaired unsafe numeric-PID cleanup, missing early child-exit
detection, invalid IPC names derived from dotted paths, existing-case overwrite,
and malformed base64 differences. Real process regressions cover pidfd ownership,
unowned namespace preservation, blocked-pipe termination and trace-append signals.
The original error bounds and assertions remain intact.

After deleting all seven journal/bootlog Python files, the combined 67 tests
passed, and the parent reran complete frozen journald IPC comparison without a
Python interpreter in PATH. Combined strict Clippy also passed. The support and
hardware CI jobs require native builds, these tests and strict Clippy; all 177
inherited disk-guard argument records remain unchanged, with two additional
2 GiB reservations. Inventory is now 126 replaced-local / 3 partially-replaced /
3,451 pending. Model compiler and repository-wide completion gates remain open;
no device result is claimed.

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

### Native network screen reference and checker (2026-10-18)

The source-independent network reference preserves 66 non-environment and seven
manual-loop environment contracts, with exact original PNG bytes, decoded pixels,
state histories and admission failures. Nineteen checker process contracts pass.
The drift guard verifies 57 original source/asset pins, dependency identity, font
namespace and helper cleanup. Missing retired oracle files alone are accepted;
changed reintroduced originals and missing or changed live dependencies fail.

Initial differential failures exposed incorrect divider intensity/checkmark
placement, omitted direct-font scaling, substring membership for saved SSIDs and
missing C-float rounding of expanded description height before scroll physics.
The independently captured expectations were retained throughout the fixes;
all 73 reference cases pass. No production Wi-Fi/network policy changed.

Both Python entrypoints are removed, and shared framework orchestration invokes
`check-ui-network`. On the rebuilt current candidate, all ten scenes/67 frames
match exactly after removal, with PATH disabled. Execution tracing records only
31 native executions (checker, ten references, ten raw drawing helpers, ten
candidates). The original screenshot/CPU-dependent Confirm CI failure remains a
separate unresolved gate, covered by the temporary diagnostic described above.
Inventory including ISP's partial CLI retirement: 98 replaced, six partial,
3,476 pending. Full repository conversion and physical device checks are not done.

### Child environment removal for native camera validation (2026-10-18)

The original camera lifecycle runner removes three ambient test-control variables
before launching each lane. `CommandSpec::env_remove` now preserves that behavior:
removal follows the environment overlay and changes only the child. The default
empty list preserves all existing callers. The missing-field regression failed
before implementation; all 26 process tests and scoped strict Clippy pass after it.
The regression verifies overlay removal, unchanged parent state and inheritance
by a subsequent unrelated child. This is a shared QA process capability, not a
change to camera process settings or runtime selection.

### Native complete camera lifecycle comparator (2026-10-18)

`check-camerad-camera-lifecycle` preserves all 32 normal and eight cleanup
scenarios, including expected original aborts, image descriptor normalization,
float32 event bits, resource ownership and narrowly allowed extra cleanup.
Seven focused tests cover 43 independent process/failure contracts, 104 helper
cases, eight CLI cases, the three migrated portable artifact-path assertions,
and real raw-path/stdin/environment/namespace behavior. All 24 tests across the
camera lifecycle, kernel, sensor, ISP and source routing targets pass after
Python deletion, along with scoped strict Clippy. The initial enum name and
surrogate-hostile test parser errors were corrected without changing expectations.

The full native comparator runs both current C++ and Rust candidates with PATH
disabled and passes 32+8 cases. Its full reports match independent originals,
except the separately measured free_bytes observation; the 26 GiB guard remains.
The CI dispatcher now selects and fingerprints the native executable. The old
lifecycle CLI, kernel digest/core helpers, ISP normalization helper and Python
artifact-path test are removed after migrating every remaining caller. Python
cleanup audit remains because the continuous runtime checker still imports it.
Inventory: 102 replaced, four partial and 3,474 pending. These are host fixture
results, not device boot/recording/upload or full repository conversion evidence.

### Same-runner Confirm raster evidence (2026-10-18)

The immutable original and native Forms reference both produced PNG SHA256
`48ced25f770ec37f92456edf4ef800dc7d90f8e9127f4bb67f236366e1606df8` in
[the b1769c0b UI job](https://github.com/bin9208/openpilot-rust/actions/runs/39082659846/job/117542952197).
Decoded pixels and state are exactly equal (zero differing pixels). The original
itself differs from the locally captured `35a7` fixture, establishing that the
failed cross-environment fixed-raster assumption is not observed native drift.
The precise renderer-environment cause and a portable exact regression remain
under investigation. No alternate-hash allowlist or pixel tolerance was added;
the CI failure and temporary diagnostic are still open work.

### Portable exact Confirm regression and CI routing repair (2026-10-18)

The admission test now replays immutable original drawing calls through the same
mechanical renderer used by the native reference, for the one scene whose fixed
PNG changed across environments. It binds original source, scene, complete state,
all six fonts and eleven exact float32 measurements. PNG comparison stays exact;
a valid image with one changed RGB sample is rejected. Production rendering and
other fixtures are unchanged. Five admission tests and all 33 native CI routing
tests pass. The focused replay/mutation case also passes with SIMD disabled,
where the original raster differs; scoped strict Clippy passes.

The f3112c2d Fast check failed because its full build-command assertion omitted
both newly required native network tools. That assertion now includes the two
binaries, preserving its mandatory invocation check. The same SHA's workspace
job failed a module-order rustfmt check, corrected by ordering sensor generation
before lifecycle. Local format/diff checks pass. Fresh remote validation remains
required; the temporary immutable-original CI diagnostic is retained until then.

### Additional mandatory UI test routing repair (2026-10-18)

The exact `54ea739c` Fast run failed `forms_native_reference_runs_real_pixels_with_owned_display_and_raw_helper` because its complete test-command assertion still omitted the migrated `ui_network_check` and `ui_network_reference` targets. The workflow already required both. The assertion now includes them and retains all ordering, display and raw-helper checks. Fresh local tests pass: four UI routing tests and all33 CI isolation tests; `git diff --check` passes. Remote UI admission now passes the portable Confirm comparison but fails the `cursor-bool` fixed PNG comparison. This separate raster investigation remains open; no tolerance or assertion was removed.

### Native camera state and runtime stress comparators (2026-10-18)

`check-camerad-state` now preserves dynamic schema decoding and the original
input/process/artifact phases. Independent captures cover eight frame-count
boundaries,42 process cases,20 CLI cases, schema types/pointers/wire boundaries
and four lazy Event-lookup cases. Review reproduced eager Event rejection and
wire-membership failure ordering; regressions now retain the original phase and
partial outputs. Native Cap'n Proto supplies the dynamic schema boundary;
there is no fixed-schema shortcut or Python process in the replacement.

`check-camerad-runtime-stress` preserves54 policy/parser cases, actual libc errno,
preload/QEMU command construction and independently bounded BOOTTIME checks.
Thirty original process captures plus12 admission and seven lexical-path CLI
captures preserve successful and failed artifacts. Independent review reproduced
relative-path/PATH selection and false-like errno fallback differences; both are
fixed without changing original expectations.

After removing both Python checkers and migrating their CI callers/fingerprints,
all74 tests across15 camera tool targets pass. Focused strict Clippy passes.
The actual PATH-disabled stress comparator passes54 cases/151 observations;
431 artifacts match the original exactly after output-directory normalization.
Its report matches except the independently checked live clock values. The
state comparator's current C++/Rust1,080-frame proof matches all original input,
output, decoded field and receipt artifacts with PATH disabled. An initial
manual invocation omitted the SDK environment and failed loading libcapnp;
sourcing the documented native SDK environment restored the original comparison.

Inventory:104 replaced, four partial,3,472 pending. The older generic stress
checker and continuous camera runtime/cleanup checker remain pending, with their
different contracts retained. UI application lifecycle and model compiler work
remain uncommitted and incomplete. No complete repository conversion, device
boot/recording/upload or CPU improvement is claimed.

### Standalone camera stress command (2026-10-18)

The older `check-camerad-stress` is now native, retaining its arbitrary runner and
profile, strict policy stderr equality, inherited real-clock child environment,
and original report order. It shares transport/comparison mechanics with the
continuous checker while keeping those differences explicit. Independent captures
cover23 process contracts,19 actual CLI cases and three raw-byte runner cases.
Additional original comparisons exposed Unicode-digit/newline negative-runner
classification and a raw-byte argument containing a space; regression tests failed
before the focused fixes. Default runtime checker regressions remain unchanged.

All four standalone and five runtime stress tests pass; four source/routing tests
and scoped strict Clippy pass. The original current fixture's stricter stderr
failure is reproduced exactly, including95 partial artifacts. A separate positive
checker test uses mechanical shell adapters to the original C++ policy/parser and
the native real clock: both original and native checker pass54cases/151observations
and431 artifacts match, with each clock independently bounded. This proves checker
behavior, not an additional source/native camera policy equivalence claim.

The single Python entrypoint is removed after confirming no remaining source
callers. CI requires the native binary and tests. Inventory:105 replaced, four
partial,3,471 pending; continuous camera runtime/cleanup remains next work.

### Portable exact admission raster comparison (2026-10-18)

The cursor failure in `497c011e` Rust CI is a fixed-raster environment mismatch:
the original drawing calls and complete state remain stable across timing probes.
The independent immutable corpus now covers all44 successful admission images,
retaining original source/input/font bindings and exact font measurements. Each
native image is compared byte-for-byte with original calls rendered in the same
environment. The remaining21 success/error cases retain their prior assertions.
Mutation tests reject changed inputs, captures and a single changed RGB sample.

All six admission tests pass in normal rendering and with SIMD disabled; the
parent's fresh full rerun passes in199.82seconds. Independent original capture and
replay comparisons pass44/44, with12 timing/renderer controls. Strict scoped
Clippy passes. Remote verification remains required; no production widget policy
changed and the temporary original diagnostic remains until that gate passes.

### Final sensor/exposure Python helper closure (2026-10-18)

The migrated state checker was the final maintained caller of sensor f32/equality
and exposure quoting helpers. Repository-wide caller search found no remaining
imports or executions, so both residual Python files are removed. Their original
structural comparison/error and quoted-input fixtures remain unchanged.
After deletion, all19 sensor/exposure/state tests pass. A fresh PATH-disabled
1,080-frame current C++/Rust state comparison passes and all eight original input,
output, field and receipt files match exactly. Inventory is now107 replaced,
two partial and3,471 pending. The remaining partial entries are the alert and
selfdrive helper loaders; the continuous camera controller is separate pending work.

### Native continuous camera verification and CI orchestration (2026-10-18)

Three Python files are removed after their native callers and regression coverage
passed: camerad_runtime_check.py, camerad_camera_lifecycle_cleanup.py and
check_camerad_ci.py. The native controller uses real Msgq, VisionIPC and ZMQ and
projects the complete current Event schema through native Cap'n Proto. Production
checks read current sources and run current C++/Rust binaries, not captured answers.

Independent expectations preserve60 original verifier/coercion/log-type contracts,
25 controller success/failure cases and12 actual CLI cases. Review reproduced
reaped-PID signalling and descendants surviving explicit wait; native process-group
termination now occurs while the leader PID remains reserved. Both regression tests
failed before repair. Raw log equality, prefix errors, list/dictionary membership
and short-circuit semantics have eight additional original captures and observed
RED/GREEN evidence. No earlier expectation or runtime criterion was weakened.

Per-message Cap'n Proto subprocesses reproduced a live timeout that the original
passed in the same environment. Finite raw-message batches now use one conversion,
with per-row fallback on malformed or partial output. An original receive-error
capture protects the successfully decoded prefix and reproduced an initial batching
bug before repair. All eight live cases pass without Python on PATH; original and
native verification of the same captures produced identical report, stdout,
cleanup and oracle artifacts. This is host fixture evidence, not device acceptance.

The native CI orchestrator preserves58 ordered build, constant, policy, lifecycle,
state, stress and continuous-runtime commands, including disk guards, source/binary
receipts and partial artifacts. Original captures include eight child failures,
low-disk refusal, two constant mismatches, five preflight cases, nine actual CLI
cases and a real missing-compiler launch. Source hashing uses a bounded64-KiB buffer.
The mandatory camera job now builds/tests and invokes the native commands.

Verification: all58 actual commands returned zero, final status PASS, and all37
preflight executable hashes were unchanged after execution. Source/routing checks
passed42 tests. After removing the three Python files, the affected regression gate
passed49 tests and scoped strict Clippy. The current-source camera packages were
rebuilt, then all eight live cases passed again in43 seconds using the just-built
C++ source fixture/oracle and PATH=/nonexistent. The final rerun was scoped to the
changed verifier and rebuilt daemon; unchanged C++ build stages were not repeated.
Local evidence: camera-ci-check/{live-native-v4,post-retirement-tests.log,
post-retirement-clippy.log} and camera-runtime-check/live-post-retirement under the
2026-10-18 scratch directories. Remote exact-commit validation remains pending.

Inventory is110 replaced-local, two partial and3,468 pending baseline sources.
Remaining comparison/build Python (including the separate VisionIPC peer builder),
UI application lifecycle and full model compiler work remain incomplete. No whole
repository conversion, device startup/recording/upload, or CPU savings is claimed.

### Camera CI Python dependency closure (2026-10-18)

Current VisionIPC tests already use unchanged original C++ peers and native Rust
clients/servers. After continuous-controller migration, the camera job's Python
setup, PYTHONPATH, pip packages, Cython binding builds and import probes had no
remaining consumer. Those camera-only steps are removed; generic binding builders
remain for their other actual workflow consumers. Existing native package/build/
Clippy/driver comparisons remain mandatory, with vision_native interoperability
added beside vision and vision_server.

Two new policy tests first failed on the retained PYTHONPATH, then passed after
the workflow cleanup. Their mutations reject restored Python setup/imports/packages
and removed, skipped or soft-failing native coverage. All40 routing/isolation tests
and strict Clippy pass. Freshly built original C++ VisionIPC and Rust peers pass all
three actual process/FD/layout/retention/stream tests with PATH=/nonexistent and
Python environment variables removed. A scratch shell harness initially allowed
one test to consume the TSV loop's stdin; rerunning each executable with /dev/null
stdin completed all three. No production transport behavior or tests were relaxed.
Evidence:2026-10-18-vision-peer-closure, including Cargo-selected executable paths
and SHA256 records. This closes Python usage in the camera job, not other jobs or
repository-wide Python retirement. Exact-commit cloud results remain pending.

### Native encoder codec comparator (2026-10-18)

The codec checker is replaced by `check-encoder-codecs`. It extracts current C++
FFmpeg/JPEG method bodies, compiles a live source oracle and compares all packets
and metadata; fixtures are test evidence only. Fourteen independently captured
original success/failure paths preserve command ordering, environment, stdout and
complete/partial artifact bytes. The native implementation passed those cases,
120-second child deadline/stdin contracts and scoped strict Clippy. Source/library
SHA256 now uses a bounded64-KiB buffer rather than reading entire files at once.

Fresh Cargo-selected encoder/JPEG artifacts passed all five actual codec cases,
112 packets. The original and native checkers produced identical full receipts
when run at the same output path. Initial distinct-path receipts differed only in
the source executable hash: C++ assertion strings embed the source filename.
Generated source and all codec results were already identical; no hash was dropped
from the comparison. Same-path rerun proved the complete receipt contract.
The native CI caller fingerprints the new binary/source and requires its tests,
Clippy and build. Runtime/V4L Python checks remain separate pending conversions.

Exact9c035f85 Fast checks exposed a stale disk-guard snapshot after camera-only
pip/Cython steps were removed. An independent old/current guard comparison confirms
only their512/1024-MiB guards were removed; all177 still-used baseline guards are
unchanged. The new encoder test/Clippy each have separate2048-MiB guards. Existing
native camera operations retain their reserves; the VisionIPC test adds coverage.
The count/hash contract is updated to these actual operations, not relaxed.
Local evidence: `.analysis/scratch/2026-10-18-encoder-codecs/`, including original14
contracts, actual source/native receipts and disk-guard-delta.json. Post-removal
and remote verification results follow; device acceptance remains user-owned.

After deleting the original codec checker:48 affected native tests passed
(13disk bootstrap,33routing/isolation,2codec tests covering14original and5CLI
cases); scoped strict Clippy passed. All20 retained encoder caller/outcome tests
passed. The actual five codec comparisons were rerun after deletion and passed
112packets again. New Rust files pass the workspace's2021edition formatting.
Inventory now records111replaced-local,2partial and3,467pending baseline sources.
The full repository and device acceptance are not complete.

### Encoder V4L verifier and codec path phases (2026-10-18)

The V4L comparison has a native implementation that still compiles current
source methods and the actual scripted ioctl/memory driver. Before implementation,
17independent original main captures preserved all32success cases and failure
artifacts across build/disk/process/trace/count/rotation/config/metadata/expected
abort/sanitizer boundaries. Separate19original outcome cases preserve exit status,
required diagnostic and sanitizer rejection. Native tests match full command,
environment, stdout, cleanup and artifact bytes for all captures; Clippy passes.
Fresh Cargo encoder_v4l_trace and current C++ pass all32actual driver cases.
Original/native checkers run at the same output path produce byte-identical complete
receipts, including source/driver executable hashes. This is host fixture evidence,
not hardware/device acceptance. Python checker removal/CI migration is prepared;
post-removal gate is next before commit. Encoder runtime/CI remain pending.

Independent review of the prior codec conversion found eager path resolution
changed failure ordering for looping--rust/native/jpeg symlinks. Four actual
original CLI phase captures reproduce the difference; pre-fix native output
fails them. Resolution is now at the original source-extraction, pkg-config,
build/child and final-hash phases. Original14contracts and fournewCLIcases pass
with strictClippy. No earlier expected result was changed.

Post-removal V4L gate passed51native tests (13disk bootstrap,33routing/isolation,
3codec and2V4L; the latter retain17main/19outcome contracts). Scoped strictClippy
passed, and all32live source/Rust V4L cases passed again without the retired checker.
All20retained encoder CI/outcome tests also pass. Inventory is112replaced-local,
2partial,3,466pending baseline sources. Remaining encoder runtime/CI is still Python.

Exact8752ce82 cloud Fast checks passed. Rust run39093856907 has an actual UI
family-pixel failure; current raster-portability repairs are still being validated.
Do not interpret other successful jobs or the local V4L gate as whole-branch success.

### Encoder runtime controller closure in progress (2026-10-18)

Independent original captures cover26actual full-cereal packet cases,18exercise
schedules/failures,11orchestration outcomes and5cleanup precedence paths. Native
controller tests and three actual owned-child shutdown cases pass; scoped strict
Clippy passes. The initial native projection admitted unknown enum8, whereas the
original to_dict raises Member was null.; an added unchanged-source control
reproduced the mismatch before the native rejection was fixed. Rotation assertion
order now follows original service order instead of sorted map order. A harness
represented the zero-second pause as0.0 instead of original0; only that recorded
call representation was repaired, with original captures untouched.

These units do not retire the runtime checker: actual VisionIPC/Msgq peer integration,
all10live original/Rust cases, CLI/CI migration and source deletion remain pending.
Evidence:2026-10-18-encoder-runtime/{controller-green4,controller-clippy}.log,
encoder_runtime_wire-red.log and source-bound corpus. The user retains device
boot/recording/upload acceptance; no current device result is claimed.

### Exact UI raster comparison across renderers (2026-10-18)

The fixed host PNG digests caused real remote family failures despite unchanged
source drawing. Test-only oracles now replay independently captured original
raylib calls on the current renderer, requiring exact PNG bytes, decoded array
samples and recording frames. The original fixture digests remain provenance;
there is no pixel tolerance or accepted-hash list. Whole-fixture, lossless input,
source, font, atlas, asset and measurement bindings reject drift. The production
widget implementations do not read these captures. Two adapter defects (startup
without a PNG and one-ULP serde reserialization) were reproduced and repaired
without changing the immutable original fixtures.

All385 independent original replays match their original raster evidence. The
current family7, admission6, Forms10, Render5 and Network4 tests pass32/32 with no
failures or ignored cases; scoped strict Clippy passes. Parent inspection checked
the twelve changed source hashes and all five final test-log hashes/results.
Evidence:2026-10-18-ui-network/raster-final-results.json and portability handoff.
Fresh exact-commit cloud verification is still required; previous UI failure runs
are not passing evidence. Python retirement count is unchanged by this repair.

### Encoder live-oracle JPEG ABI and input fidelity (2026-10-18, #352)

The first native runtime comparison rejected the source process after its first
thumbnail: the linked Cargo JPEG archive uses ABI62 but SDK headers selected ABI80.
Actual g++ preprocessing confirmed SDK jpeglib.h and multiarch jconfig.h selection.
The source builder now places the pinned JPEG source/configured headers before
all dependency include paths. A dedicated ordering regression failed before the
fix; all7source-builder tests pass afterward. Immutable original captures remain
unchanged, with only the intended two-header ordering migration in test adapters.
Full corrected-source live comparisons and strict Clippy remain pending.

Independent review also found the new comparator sent VisionIPC valid=true while
the unchanged Python wrapper sets false. An actual native receiver test reproduced
the difference; it now verifies false, frame ID, timestamps, buffer index and all
pixels. Native missing-binary and actual spawned-/bin/false tests verify namespace
cleanup and reaping. These are checker fixes, not changes to driving validity.
UI6mode host proof passed before caller retirement; post-removal verification is
still in progress. Issue352 and issue296 retain the pending acceptance boundaries.

### Native application lifecycle tools retired (2026-10-18)

`check_ui_application.py` and `ui_qa/application_source.py` are removed after
independent native reference/controller/recording/XTest implementations and real
six-mode comparisons. The framework selects check-ui-application, with original
argument tails, stage order, failure propagation, logs and ledgers preserved.
The UI job builds the reference/raw/native checkers and runs required lifecycle
suites under its existing display. Fast checks require lifecycle routing mutations.
All220existing disk guards remain; three new guards reserve16/512/512MiB.

Post-removal native policy, loop, owned child, raw input, recording, source guards,
framework and workflow tests pass. The source-guard fixture initially failed while
reintroducing a removed tool into a missing parent directory; creating that test
directory repairs it while retaining changed-source rejection and requiring all
other pinned sources. Strict scoped Clippy and all three checker builds pass.
The six real modes pass again with no Python executable on PATH: original
five-mode pixels/state, three-frame global/dynamic videos, real keyboard text
native and first-click shutdown,37frames/ticks, and native process/log identity.
These are host UI checks, not comma boot/record/upload acceptance.

Evidence:2026-10-18-ui-application-lifecycle/post-retirement-{tests,drift-green,
remaining-green,clippy,build,six-mode-proof}.log. The baseline inventory now has
114replaced-local,2partial and3,464pending sources. Full conversion, remaining
model/compiler/tools and exact-source final remote validation remain open.

Parent final verification reran all13current native test executables:86tests
passed with zero failures/ignored cases, including all6reference tests together,
workflow policy and fast routing. Independent staged-source review found no
encoder dependency in this UI commit; staged Rust formatting and diff checks pass.
Evidence:2026-10-18-ui-application-lifecycle/parent-final-tests.log.

### Native encoder host CI and runtime tools retired (2026-10-18)

The native runtime peer reproduces all ten current C++/Rust daemon comparisons,
including multi-stream, rotation, activation, listener restart and lag overwrite.
The complete native orchestrator built the current source and passed all5codec,
32V4L and10runtime cases using a PATH containing only the required native tools.
The source, eight executables, complete cereal/IPC identity, current Cargo build
outputs and native codec versions are recorded; no Python interpreter is selected.

The original CI/runtime/outcomes tools and two encoder test files are removed.
The mixed CI test file's four Params contracts remain unchanged in
`test_params_native_stager.py` and are required by the existing Xiaoge source job.
They passed4/4 against this worktree. That inventory row remains partially replaced.
The encoder job removes Python setup/packages/bindings; native process, wire,
schedule, shutdown, receipt/failure and source tests now replace those checks.
All223existing disk reserves and all33Rust job isolation contracts pass.
Fast checks require the new native encoder routing test.

Independent review identified an unconditional retired-source read in the wire
corpus test; deletion reproduced the failure. Only the retired source's absence
is now allowed, while any changed reintroduction and current schema drift fail.
An immutable SHA binds the entire26case corpus. Parent post-deletion gate passed
86unique native tests with zero failures/ignored cases and strictClippy, including
actual wire validity/pixel reception and real failed/timeout child cleanup.
Earlier native CI tests caught a trailing separator from joining an empty suffix;
the implementation now preserves Python Path spelling without changing captures.

Evidence:2026-10-18-encoder-ci/{full-native,combined-tests.log,
combined-clippy.log,post-delete-wire-red.log,params-remnant-test-green.log}.
Post-deletion full47case repeat also passed with no interpreter on PATH; receipt:
2026-10-18-encoder-ci/post-retirement-native/receipt.json. Independent final
source review confirmed all five deletions, retained Params AST equality, full
native CI routing and source/corpus guards; no blocker remained.
Baseline inventory:118replaced-local,3partially-replaced,3,459pending. The ARM
artifact checker, model compiler and all remaining source families are pending;
these host gates do not establish physical boot/recording/upload or CPU savings.

### Encoder stager routing regression (2026-10-18)

Exact head `0bdc822d3d8594bc2f74f191ec1738e64a4adac3` failed the native
stager-routing test in [Fast checks](https://github.com/bin9208/openpilot-rust/actions/runs/39098902331).
Its caller scanner still recognized the removed Python encoder command only,
so it counted21of22callers. The scanner now recognizes the native executable
as the command token, without mistaking Cargo's `--bin check-encoder-ci` build
argument for an invocation. The required count remains22, and every missing
stager action/argument mutation remains rejected. All three routing/action
tests pass locally. No runtime behavior, CI gate or assertion count was removed;
fresh exact-source remote checks remain required.

### Complete native UI feature guard (2026-10-18)

The `75ea1957` Fast job advanced past the corrected stager scan, then exposed
a second stale routing assertion: `ui_egl_ci` still expected exactly the two
former EGL/polygon features. The same build now also requires the native
application lifecycle feature introduced in `b90c650f`. Its exact feature-list
assertion includes all three, retaining every binary, test and optional-dependency
check. Failure: [Fast checks run39099687404](https://github.com/bin9208/openpilot-rust/actions/runs/39099687404),
job117593220676. The focused routing test and strict Clippy pass.

The complete native fast-routing Cargo command passed all57 suites:350 tests,
zero failures and one pre-existing ignored child-process helper. An earlier run
failed intermittently in the actual owned-display test; that separate failure
remains under investigation, not resolved by this successful repeat. Evidence:
`2026-10-18-encoder-target/{fast-routing-native-repeat,parent-final-clippy}.log`.
The exact75ea integration workflow39099687447 passed; its Fast gate did not.

### Owned-display test executable race (2026-10-18)

Added diagnostics reproduced the intermittent display failure before child startup:
`execve` returned `ETXTBSY` (26), with an empty Xvfb log. Concurrent test forks can
inherit another thread's open script writer until exec; closing the writer in its
own thread does not close that inherited descriptor. A deterministic isolated
fork reproduced the same error after the parent closed its writer, then executed
successfully after the retaining child exited.

The test fixtures now execute an immutable Bash launcher and read each unchanged
per-test script body as data. A regression deliberately retains a body writer,
asserts that direct execution fails with26, and requires the launcher to preserve
the display report and child cleanup. The launcher is executable0755 and selects
its body only from its own private symlink directory; no environment override is
introduced. Production spawning, display parsing, all deadlines, receipt and
stream assertions remain unchanged; no retry masks original spawn failures.
The combined gate passed all13active UI tests (one existing private-display test
remains explicitly ignored), and strict Clippy included this target. Ten complete
executable repeats in two parallel workers also passed:130active test executions,
zero failures. Evidence:
`2026-10-18-ui-display-review/{diag-3-13.log,inherited-writer-proof.json,fixed-*.log}`
and `2026-10-18-encoder-target/final-findings-{tests,clippy}.log`.

### ARM encoder validator retired (2026-10-18)

`check-encoder-target` replaces `rust/tools/check_encoder_target.py`. It retains
all five source/Rust codec comparisons, exact packet counts16/16/68/6/6, archive
and header pins, explicit AGNOS loader containment, version diagnostics, final
artifact hashes, failure receipts and child deadlines. Native receipts also
identify the executing checker ELF, covering its compiled helper dependencies.
Original captures cover11 ELF/loader cases,23 full runs, five command phases,
two log-open errors and two raw Unix hash arguments. Full-run failure contents
are compared exactly; invalid UTF-8 hash arguments retain surrogateescape text
and fail at the original recorded hash-validation phase.

Separate stdout/stderr logs now go directly to files while the child runs.
Real-child tests require17MB output to be visible before child exit, preserve
timeout bytes, reap owned processes, and retain stdout-before-stderr open order.
An output-open error reports the actual failed log path. Existing merged-output
and inherited-stream behavior remain covered by the same process runner suite.

The fresh frozen ARM probe and matched JPEG archive passed both original and
native host-QEMU gates. Runtime ELFs were extracted independently from the pinned
raw AGNOS image after verifying its hash, then checked against extraction receipts.
All887 probe-source hashes and18 checker-policy/build-input hashes were unchanged.
The new checker ELF SHA256 is
`0d523a3cab8dcfcc0e52f11343cb4808bc43be0307456606b55ffd4c33a6c6c9`.

After removing the Python tool and its synthetic test-root placeholder,
103 native tests passed with zero failures and one existing private-display test
ignored. Strict scoped Clippy passed before deletion; only that unused test-root
placeholder was subsequently removed. The actual native ARM gate passed again
after deletion:5 cases,112 packets and234 byte-identical trace/packet files
against the original run. Its20 exec calls contain no Python/PyPy interpreter.
Preparation scripts remain private evidence tooling, outside the native execution.
The existing encoder CI step builds, tests and lints the new checker with all223
disk guards and33 isolation tests retained. No physical device result is claimed.

Evidence: `2026-10-18-encoder-target/{final-findings-summary.json,
post-delete-tests.log,post-delete-arm.log,post-delete-arm/receipt.json}`.
Inventory119replaced-local/3partial/3458pending; complete repository conversion,
model compiler and final exact-source integration/release gates remain open.

Independent final review found no concrete blocker in the target module/CLI,
five immutable corpora, separate-file process API, Cargo/CI routing or deletion.
No live caller selects the deleted Python tool; source, test and workflow diff
checks pass. This review does not clear the pending model/compiler work.

### UI connectivity CI diagnostic gap (#354, investigation open)

At exact commit `da803825c62d2caa9eae3d8aefb6cd579dd0791e`, Rust run
39103294926 / UI connectivity job117604082085 failed the original raw replay for
`input-list-negative-max`: GLFW could not open X11 display `:99`. This happened
before pixel comparison. Existing mismatch-only artifact retention did not keep
the failed replay's process output or server lifecycle log.

A bounded independent host probe completed88 actual GLFW runs in each of the
normal-reset and `-noreset` modes, including16 total replays of the exact failing
case, without a connection failure. This does not identify reset as the cause or
prove `-noreset` a fix. The diagnostic follow-up retains Xvfb stderr and original
replay failure context without retries or relaxed assertions. Four focused
process tests pass after reproducing the lost-evidence failure: nonzero exit,
spawn failure, partial timeout output, and preservation of the original
validation error when retention itself fails. They also verify no retry and
unchanged success behavior. Strict Clippy passes across five consumers, and
all34 workflow isolation tests pass. The entire affected list-input/setter/touch
graphics test passes with frozen binaries and real GLFW replay in158.57seconds;
this is a host rerun, not proof of an X11 root cause or a permanent fix. Fresh
exact-commit CI remains required. Local probe evidence is
`.analysis/scratch/2026-10-18-ui-display-connection/glfw-probe-summary.json`.

### Original msgq binding precedes workspace source imports (#356)

The same `da803825c62d2caa9eae3d8aefb6cd579dd0791e` run failed workspace
job117604081791 because the real selfdrived source-oracle import required
`msgq.ipc_pyx` before the workflow built it. A fresh local subprocess reproduces
that import failure with the job's original PYTHONPATH and succeeds with the
existing actual binding prepended. All four tests in the affected original
source test file pass with that real binding; no fake messaging module or skip
was introduced.

The workflow now builds its existing shared binding once before the first source
pytest step and prepends its directory for that step. Later consumers reuse the
same output. A new ordering regression fails before the repair and passes after
it. All 35 workflow isolation and 13 disk/bootstrap tests pass; the added 512 MiB
reserve raises the explicit guard count from 225 to 226 while preserving all old
guard arguments. The unchanged full source suite and downstream steps require
fresh exact-commit CI. The shared Python binding remains a tracked #296 migration
item; this repair does not claim its retirement.

### Timed QA retirement and worker cleanup (#296, #358)

Thirteen Python entrypoints/helpers were replaced by eight native checkers,
three native helper binaries and independent dynamic source references. Original
captures cover policy54, datetime360, HTTP44, logging2, daemon4, date-daemon16,
lifecycle7 and shutdown6. All493 cases pass before and after deletion using the
same frozen eleven QA ELFs and exact original candidate pair. Stable output
ordering, hostname and build metadata remain compared; timing fields are
projected only after their original bounds pass. Each run inspects1,025 execve
attempts with zero Python and audits27 worker receipts and removed IPC objects.

The final post-deletion gate passes118tests across22integration targets plus
11binary targets, followed by strict Clippy. CI preserves all unrelated support
commands and makes the17timed regression targets and all eight native checker
invocations mandatory. The13retired paths alone may be absent: changed present
sources and missing/changed live dependencies still reject. Source provenance
uses existing Git blobs; retired Python is never restored or executed.

Three independently reproduced cleanup failures are tracked in
[#358](https://github.com/bin9208/openpilot-rust/issues/358): preflight and CLI
children escaping the worker group, and a failed launch consuming a stale
ownership receipt. Inner capture now inherits the owned outer group, and cleanup
requires an invocation-bound atomic receipt. All three regressions fail before
and pass after the repair; the existing30default process-runner tests still pass.
The IPC-creation-to-first-ownership-flag crash window remains explicit. This is
QA-tool equivalence, not a new runtime release, device acceptance or CPU result.

Local evidence: `.analysis/scratch/2026-10-18-timed-tools/`. Frozen bundle snapshot
SHA256 `0514a06641b8f2deedcebf888c67ad4ec927902f9318fa8eaedba20a2849b054`;
original archive `a4f95aa7d519766cd444d61f303837884661ab2fd92c04ddf35df31aace8e6fc`;
before proof `e61b38ff33e34a16db4184903d7e7dc0d59d9c4bd7d29cf2f6ab26f246b2d6ef`;
after proof `9c5050e649794e40032577ec06f32d77d5964fa594bfc3608ce88fe5959b528d`.
Exact committed-SHA CI remains required; #296 stays open.

### Native batch matrix plus channel bias (2026-10-18, #296)

The actual driving-model node275 prefix previously failed CPU IMAGE admission
when its MatMul result fed a final channel-bias Add. The native compiler now
keeps that owned reduction and bias in one four-buffer LLVM call, with bias
strides broadcast across rows/batches and a dense f32 output. The original
operand order, half input/weight/bias storage and reduction flags remain intact.
Published/shared intermediate reductions and the older single-row API retain
their admission fences. The indexed input ownership check identifies the raw
reduction, preserving its existing single-reader exception.

Independent cold original captures for pinned LLVM18 and LLVM20 each compiled
96 distinct programs and executed251 program calls. The actual275 output hash
is8fd83b71ca2930d27637d0f48da6c703c8c332f476e06921a69e35e8ca4c3a5e;
both native memory policies match. Each provider also passes36 final-output
comparisons covering finite/negative-zero/NaN payload patterns across six shapes,
six shared/published-reduction admission negatives and the older API fence.
The affected strict Clippy gate passed. Independent source review found no
blocking defect in this bounded change. Full16-target/74-test graph regression
passed under both LLVM18 and LLVM20 with zero skipped/filtered cases.
Source and provider identities are checked before/after each full gate.

Evidence: .analysis/scratch/2026-10-18-batched-matmul/
{batched-bias275-final-proof.json,bias275-graph-gates-YyYyhUNW/proof18.json,
bias275-graph-gates-YyYyhUNW/proof20.json};
independent review .analysis/scratch/2026-10-18-model-review-image/
batched-bias275-review.md. Exact committed-SHA CI remains required.
This bounded prefix does not complete either model compiler, node277, Python
retirement, target execution or device acceptance. The LLVM20 CI command and
policy test explicitly include compiler_batched_bias.

### Checkout QA tools retired with raw Unix receipt parity (#296)

Seven checkout Python tools are replaced by a feature-gated native checker and
independent source reference. All nine metadata/Git/status/helper/inherited-exec
stages remain mandatory. Independent original captures bind 43 metadata input
byte strings, 44 state/f64/read-count rows and 12 CLI controls. Review reproduced
and fixed a non-UTF8 filename panic and excessive inherited-environment logging:
receipts now preserve arbitrary Unix bytes and the original environment allowlist,
while child environments remain unchanged.

Before and after deletion, full runs match all seven original policy outputs
and input transcripts on both sides, 11 complete helper outcomes and six original
inherited-exec outcomes. Twelve original failure phases and raw-path/environment
controls pass. Cleanup checks cover 17 peers, 281 RPC child/descriptor checks,
22 probes and four original strict timeouts. Both final complete exec traces
contain 426 attempts and zero interpreters. Ten focused tests and strict Clippy
pass after deletion; rebuilt checker/reference ELFs are identical to the repaired
pre-deletion pair. The retained runtime update_status.py stays pinned.

Evidence: `.analysis/scratch/2026-10-18-checkout-tools/postdelete-final-proofs.json`,
`native-gate-NwarOU7j/`, `actual-native-RM7CZtht/`,
`postdelete-failure-77nn57um/` and `postdelete-raw-vunmhtzt/`.
Source fixture expectations remain unchanged. Inventory is 148 replaced-local,
three partially replaced and 3,429 pending. Exact committed-SHA CI remains
required; this does not establish complete conversion or device acceptance.

### UI raw-window failure handling and Xvfb lifetime (#354)

The real graphics helper previously reported success after InitWindow failed;
subsequent font work and cleanup could then cause SIGSEGV. An absent-display
reproducer fails with the old frozen helper and passes after IsWindowReady is
checked before marking the window owned. The real-provider regression checks
exact window/font/quit error replies and clean process exit. Existing successful
drawing commands remain unchanged. Strict Clippy and independent review pass.
With the new frozen helper, all eight network and fifteen Forms tests pass,
including exact original/native state, pixel, error and recording comparisons.
Frozen tool/provider hashes match before and after both suites.

A separate authenticated Xvfb experiment compares default reset behavior with
-noreset: 175/180 short GL clients and 35/36 exact network-list replays. All 71
images are byte-identical; keymap initializations fall from 176 to one. The main
UI CI comparison now keeps that server alive across clients, preserving all
mandatory gates and failure diagnostics. The exact CI display-open failure was
not reproduced; this is a verified lifecycle improvement and secondary-crash
fix, not proof that the underlying CI connection cause is resolved.

Evidence: `.analysis/scratch/2026-10-18-ui-raw-window/` (RED/GREEN, actual-provider
regression, strict Clippy, independent review and success-render-mPVmEiUm) and
`.analysis/scratch/2026-10-18-ui-display-connection/lifecycle-control-190718/`.
The 49 workflow/disk-policy tests and scoped Clippy also pass. Exact committed-SHA
CI remains required; no device result is claimed.

### Timed QA executable provenance precedes child launch (#359)

The live checker used to launch its child before hashing that executable for the
ownership receipt. Injecting 17 seconds at that post-launch read reproduces the
failed CI shape: all 151 response chunks arrive before shutdown, peer_closed is
false, and SIGINT still exits promptly. The exact CI hash duration is unmeasured;
the controlled reproducer establishes the ordering defect independently.

The same executable digest is now computed after opening/configuring the process
log and before starting the child. Receipt fields, child ownership, six-second
pre-signal wait, progress requirement and two-second shutdown/peer bounds remain
unchanged. Actual source and runtime controls with 17-second hash delays now pass.
A permanent real-process latency regression verifies read-before-exec ordering;
error controls preserve log-open precedence and missing-executable empty logs.
Twelve affected tests, two mandatory CI-policy tests and strict Clippy pass.
The fresh frozen eight checkers again pass all 493 original cases, 27 cleanup
receipts and 1,025 exec attempts with zero Python. Parent review finds no blocker.

The support CI job previously reached the timed tests only near its 30-minute
job deadline, before its remaining full CLI comparisons. Its complete regression
budget is now 45 minutes; debug/incremental artifacts are disabled, as in other
large native QA jobs. All original commands and per-case deadlines remain fixed.
The original CI fixture is unchanged; policy mutation tests enforce the explicit
new job budget and artifact bounds. This does not replace the ordering repair or
establish full CI success. Evidence: `.analysis/scratch/2026-10-18-timed-shutdown-ci/`
`validation.json`, `ROOT-CAUSE.md`, delayed-hash proofs and `parent-review.txt`.
Exact committed-SHA CI and user device acceptance remain pending.

### Native manager catalog QA closure (#296, 2026-10-18)

Replaced `check_manager_catalog.py` and `manager_catalog_reference.py` with the
native `check-manager-catalog BINARY OUTPUT [--runner ...]` and
`manager-catalog-reference REQUEST OUTPUT` commands. The obsolete Cython binding
argument is removed explicitly; original binding hashes remain capture provenance,
not a claim that an interpreter or binding executes in the new checker. The
independent reference derives constructors, ordered callbacks, raw Params getters
and GPS/file effects from the original source; it imports no candidate manager
catalog, Params implementation or captured-answer lookup.

Before implementation, the unchanged original passed all 116 scenarios: 63 ordered
process descriptors, 64 flag configurations, 25 environment snapshots, 10,573
nonfatal callback cases and 24 actual integer SIGABRT boundaries. Immutable
captures also cover 38 generic inputs, 20 module-global call/serialization cases,
five surrogate/NUL/integer-overflow controls and CLI/failure behavior. Missing-API
RED preceded implementation. Further observed REDs corrected missing-candidate
failure ordering and four generic exception categories; fixed expected bytes were
preserved. The native reference reports fatal_integer with the original access
prefix; it deliberately does not claim Cython signal parity.

Ten manager tests, 36 mandatory CI-policy tests and strict Clippy passed. Frozen
native tools passed all 116 full scenarios with an empty PATH, exactly matching
all original request bytes, independent logical results/fatal prefixes and all
candidate result objects. The execution trace records 233 exec attempts and zero
interpreters. Fifteen actual CLI/failure controls and five additional generic
error controls passed. Original stdout bytes and top-level report counts remain
preserved; native executable identities are separately sealed.

After updating the startup CI caller and deleting exactly the two replaced Python
tools, the parent reran all 46 prebuilt tests and the complete frozen 116-case
proof with the same exact comparisons and zero interpreters. Remaining binding
builders, original_params_binding.py and original process-policy sources remain
required for other conversion units. This is a policy-tool retirement, not actual
manager launch, normal device boot or repository-wide Python removal.

Evidence: `.analysis/scratch/2026-10-18-manager-catalog-tools/`, notably
`native-gate-eOgTC6ds/`, `actual-native-XuEoIUWD/`, post-deletion
`actual-native-rpzHv4RJ/`, `post-delete-full.log`, `cli-failures-zx6fqd61/`,
`generic-edges-native-2q3ncogm/` and `parent-review.md`. Inventory is now 150
replaced-local, three partially replaced and 3,427 pending baseline files.
Exact committed-SHA CI/integration and the user's device verification remain open.

The subsequent full disk-bootstrap policy check reproduced a stale guard-count
expectation (238 versus241) after the manager build/test/Clippy steps were added.
Its policy now separately verifies the three new reserves (1024/256/256 MiB),
retaining the unchanged original177 guard-argument hash and all existing family
assertions. All13 disk-bootstrap tests pass. Evidence is retained alongside the
manager captures as `disk-policy-red.log` and `disk-policy-green.log`.

### Native batch matrix, channel bias and final SiLU (#296, 2026-10-18)

The driving-model prefix ending at node277 previously failed the native CPU IMAGE
rank-two matrix admission fence. The compiler now owns its MatMul, channel-bias
Add, Sigmoid and final Mul as one four-buffer call. The original exp2 polynomial,
signed conversions, exception handling and arithmetic ordering remain in the
reduction DAG without an intermediate half store. Held intermediates, extra
readers, changed Sigmoid input and commuted Mul remain rejected by this bounded
final-only profile; the previous bias-only path remains intact.

Independent cold LLVM18/LLVM20 originals each compiled96 distinct programs and
executed251 calls, producing the same18,432-byte output SHA256
656d9d47f90cbf3c05ea54e2c1ff244d4326da4183ca3c35b4a7f4e345534d91.
Native final outputs match under both memory policies. Independent controls
supply38 whole-output comparisons and16 ownership/order guards per provider.
The actual native rank-two rejection was observed before implementation; no
expected answers were generated from the replacement compiler.

Both complete17-target/77-test affected graph gates pass with no failed, ignored
or filtered tests, retaining prior274/275 prefix coverage. Before/after hashes
match for all220 model source/fixture files, both actual models and both LLVM
providers. Final strict Clippy on the library, binaries and affected SiLU/bias
tests passes after immutable fixture wiring. Parent review found no blocker.
The workflow and its policy regression require the added target on LLVM20.

Evidence: `.analysis/scratch/2026-10-18-batched-matmul/`, specifically
`NODE277-FINAL-GRAPH-PROOF.json`, `silu277-graph-gates-IUOQt39I/proof18.json`,
`proof20.json`, `silu277-final-clippy.log` and `node277-parent-review.md`.
Only the README was reconciled after the frozen graph gates. Exact committed-SHA
CI remains required. Later graph prefixes, full CPU/GPU export, repository-wide
Python retirement and user device verification remain open; no CPU saving is
claimed from these correctness checks.

### Native feedback checker and cleanup closure (#296 / #362, 2026-10-18)

Replaced and deleted only `rust/tools/check_feedbackd.py` with native
`check-feedbackd --binary BINARY --output OUTPUT`. Its CLI-private Rust source
reference uses full Cereal/msgq IPC with independent bookmark policy. The source's
literal-false audio gate, invalid-bookmark handling, logging and 8,000/200/3,000 ms
readiness/subscriber/shutdown limits remain intact. The 35 retained original
source pins remain mandatory; only the actually absent retired checker is allowed.
Changed present copies and dangling symlinks are rejected.

The unchanged original checker passed against fresh daemon SHA256
43bb4229f836b75a3648c5ea0a863947645b10c4283459de812c710036f39226.
Five policy/provenance and three actual IPC/owner tests, strict Clippy and the
frozen native build passed. The complete eight-iteration native report equals
the original report: four bookmarks, no audio and four log messages. Empty-PATH
tracing records three ELF executions and no Python. Eight CLI controls and three
real child failure paths preserve raw failure evidence while reaping owned
children and removing only their IPC namespaces; an unrelated namespace sentinel
survives. This fixes the original checker's spawn-failure namespace leak (#362),
not a demonstrated feedback runtime fault. A genuine dangling-symlink guard RED
preceded the minimal actual-absence fix.

After deleting the Python checker, the same sealed native proof passes unchanged
(`actual-native-z0llkzl9`). Sensor/audio CI now builds and tests the native checker,
requires its actual freshly built daemon and runs strict Clippy before executing
it. A missing-native-caller CI RED preceded this wiring; all 36 workflow-policy
and 13 disk-bootstrap tests pass. Existing 177 guard argument hashes are preserved;
the new native builds/checks add explicit disk reserve guards.

Evidence is indexed in `.analysis/scratch/2026-10-18-feedbackd-tools/`, including
`FINAL-EVIDENCE.md`, `native-gate-Cl5efKcP`, both actual-native receipts, `ci-red.log`
and the post-deletion test log. Inventory:151 replaced-local,3 partial,3426 pending.
Audio recording, complete Python retirement and device startup/recording/upload
acceptance are not established by this host checker. The user will perform the
final device verification after pulling the completed candidate.

### Native GNSS assistance checker closure (#296, 2026-10-18)

Deleted only `rust/tools/check_qcomgps_assistance.py` after replacing its positional
CLI and all four real loopback/file scenarios with `check-qcomgps-assistance`.
The unchanged original checker was rerun against fresh daemon SHA256
6d3aaac652c0ae3feaed88cd222ce3c48227199c3ed700b03d14ceb731a24c9e;
all four result objects are identical to the independent original fixture.
The native checker retains alternate-file replacement, accepted404 body,
oversize temporary-file handling and exactly one retry, with exact file contents,
15-second observation and3-second shutdown. The runtime daemon is unchanged.

Actual missing-API RED preceded implementation. Independent review added seven
private HTTP-server regressions: GET-only dispatch, original request/header limits,
owned cancellation during slow headers, and the original-valid1.5-second header
pause. The last case failed under a newly introduced1-second socket limit; removing
that limit restores original acceptance while exact-owned socket shutdown still
bounds finish/Drop. No candidate policy or fixture output drives the live checker.
All80 library tests and7 actual integration tests, strict Clippy and build passed.

Frozen checker SHA25648d8d042304e0f666035c6b98b7bee19416fc1840aeba9502d2e9217c9a4d8c4
passes the full runtime proof both before and after deletion: four original-exact
objects, five CLI, four actual failure and five provenance controls. Empty-PATH
tracing has five execs using only two owned ELF paths and no interpreter. The
proof verifies unchanged checker/source seals and245 files in the daemon's path
dependency closure; it does not claim a complete runtime bundle. All seven actual
integration tests also pass after deletion. GNSS CI now builds/tests/lints the
native command with mandatory current-source QCOMGPSD_BINARY; workflow/disk49 pass.

Evidence: `.analysis/scratch/2026-10-18-qcom-assistance-tools/`, particularly
`header-pause-red.log`, `final-tests.log`, `final-clippy.log`,
`final-frozen-yxjmwvAU`, `actual-native-1uyubxym`, `actual-native-4gemcmt4`,
`post-delete-tests.log` and `parent-review.md`. Counts:152 replaced-local,
3 partially replaced,3425 pending of3580 baseline entries. Complete Python removal,
exact committed-SHA CI and user device verification remain open.

### Native lagd checker and reference family (#296, 2026-10-18)

Replaced and deleted the six check_lagd numeric/loop/cache/packet/daemon/Params
commands and their lagd_reference, lagd_loop_reference and lagd_frames helpers.
Native references generate and evaluate the original numerical/loop inputs;
production checks do not return precomputed candidate answers. NumPy-derived
PCG64/Ziggurat generation preserves its independent38,912-output/state proof and
licenses. The16 remaining live runtime/schema/kernel inputs remain hash-pinned;
only the nine exact retired paths may be absent. Modified present originals,
dangling links and missing retained inputs fail the source-admission tests.

All six unchanged original commands passed against the same twelve freshly built,
sealed native tool/candidate executables before deletion. Native runs match all
3,831 cases:95 numeric,3,703 complete-loop frames,14 cache,13 packet, four actual
daemon scenarios and two Params scenarios. Forty-five output artifacts compare:
31 are byte-exact; the remainder preserve the original1e-12 pose budget and
explicit source-clock fields. The121 pose differences have maximum1.33e-15;
all other inputs, fields, ordering and failure decisions retain their exact
contracts. Learning has1,201 frames/241 publications; corrupt and cadence cases
retain6/2 and11/3. No runtime validity limit or cadence was loosened.

Actual RED exposed native SIGINT exit1 and raw Params CRLF/bare-CR translation.
Cleanup now completes under retained child/session ownership before default
SIGINT termination; raw Params text stays raw while peer text keeps the original
universal-newline behavior. The setsid pre-exec helper uses only the safe syscall
boundary, and group cleanup retains the unreaped leader identity. Independent
source review found no ownership, caller, source-absence or CI blocker.

The same sealed twelve executables pass all six proofs after deletion, with34
execve attempts and zero Python attempts per proof. Post-deletion34 family tests
and53 CI tests pass, as does strict Clippy for the library, six binaries and all
18 test targets. Estimation CI builds the six native commands and mandates all15
family targets plus the CI contract before real candidate comparisons. Existing
locationd/paramsd/oracle/sanitizer commands are preserved;267 disk guards across33
jobs retain all177 original guard arguments. Only the fixture README changed
after the frozen comparisons, with old/new hashes recorded.

Evidence: `.analysis/scratch/2026-10-18-lagd-tools/final-postdelete-evidence.json`,
`six-final-1/{original-receipt.json,native-before/proof.json,native-after/proof.json}`,
postdelete-final logs and sol-model-native-owner-review.md. Inventory now has161
replaced-local, three partial and3,416 pending entries of3,580. This closes this
tool family only; complete Python retirement, exact committed-SHA gates and the
user's boot/recording/upload verification remain outstanding.

## 2026-10-19 — Version/DBC CI follow-through

The e4820a7f candidate completed all15 applicable macOS ARM64 DBC checks
(job117744085617). The PR merge tree matched the candidate tree exactly. Its
telemetry job completed27 native Version tests before failing because the minimal
Rust installation did not include Clippy. Commit5b98bcee installs that component
and retains the strict check. The fast disk-policy test then exposed three newly
added guards missing from its accounting: one DBC errno regression and the Version
test/Clippy pair. Commit6e953828 preserves the177 baseline guard arguments/hash and
accounts for all277 guards across34jobs; all13 disk tests and39 CI isolation tests
pass locally. The subsequent workspace formatting failure was the new two-line
condition in that policy test;2106ad47 applies and checks rustfmt for it.

The6e953828 Fast checks passed (run39152329117). Its Rust run39152329062 was not
successful because of the formatting failure above. No previous-commit success
substitutes for the latest exact-SHA gates. Complete CI, dev integration and the
source-matched runtime release remain open. As requested, the user will perform
boot, recording and upload acceptance after pulling the delivered candidate.

## 2026-10-19 — native SDK receiver and independent Sentry source foundation

`check-sdk-receiver` replaces `rust/tools/check_sdk_receiver.py`. The unchanged
original first passed three actual loopback POSTs (identity/gzip/Brotli); the native
command replays those immutable independent wire bytes, including the captured
gzip timestamp. Both before and after Python removal, the same sealed ELF matches
every original report field except the newly bound Host port. Empty-PATH strace
records one execve and zero Python attempts per run. This checks real decode,
headers, payload and response paths, not a prerecorded success report. The shared
Python crash transport receiver still serves pending managed-entry checks and is
explicitly retained.

Independent review exposed two real native REDs: a reset peer discarded an already
decoded event because HTTP acknowledgement preceded publication, and `--help`
created an output artifact. The source receiver queues its event first; the Rust
receiver now does likewise, preserving parse errors while retaining accepted events
when acknowledgement fails. Help exits before binding or file creation. Eight
native tests cover those cases, three codecs, invalid input and recovery, concurrent
partial clients, close/join ownership and a continuously progressing input's total
I/O deadline. All8 pass, as do13 disk-policy tests,39 CI policy tests and strict
scoped Clippy. Decoder CPU time on arbitrarily large bodies is not claimed bounded
by the socket deadline. Startup CI installs Clippy, builds the current native tool,
requires tests/Clippy and runs the actual command. All177 baseline disk reservations
remain; the two new512MiB reservations make279 across34jobs.

Evidence: `.analysis/scratch/2026-10-19-sdk-receiver/` contains original capture,
review-red/review-green2/review-clippy2 logs, frozen ELF/source hashes and
`postdelete-proof.json`; the independent reset-peer capture is also a permanent
fixture. Inventory180 replaced-local/4partial/3396pending remains incomplete.

The independent `tombstone_source` Worker has separately passed39 pure/protocol/
transport controls plus72 stateful controls (50 original Sentry gates,9 partial
configure/Params-order cases and13 typed tag calls). It uses original Version and
Params mechanics, never native runtime candidate answers. Full unchanged Worker
recapture corrected a missing-constant seam in the earlier fixture: seven false
filename AttributeErrors and one CR-only transport expectation were repaired from
source-only evidence before equivalence was claimed. All7 foundation/stateful test
target entries and scoped Clippy passed. Remaining file/scanner/retrace/daemon
cases, complete242-call coverage, controllers and shared-loader retirement remain
open. No device validation or complete Python removal follows from these gates.

### Exact-head CI and directory enumeration follow-up (2026-10-19)

At2106ad4791d488a6361f93b85a8b43eb3d2de1fe, telemetry job117752240551
passed the27Version tests,171realGit comparisons, actual collector boundaries,
and retained tombstone/runtime/SDK consumers. macOS DBC job117752240511 also
passed. Integration39153248518, FastPR39153248526 and docs39153248571 pass.
The Rust workflow39153248437 is not green: workspace job117752240654 exposed
a fixed-order expectation in the collision-enumeration DBC regression.
The tested mergeaa0f9639f882fb62de5a45160eb0d21b5c867534 has the same tree
0416b71d5b6bd76891ca58adeb144180cb3f2c6c as this head (GitHub commit API).

Unchanged original generator source independently produces FIRST on tmpfs
when enumeration is second/first, and SECOND on ext4 when it is first/second.
The frozen native test reproduces the same CI assertion under TMPDIR=/dev/shm
(exit101). A test-only correction binds exact independently captured bytes to
actual input enumeration before invoking the candidate. Both creation orders
and Linux filesystems are covered; no sorted-order production change or
indiscriminate acceptance of either output is introduced. Native GREEN and
committed CI for this correction remain pending. Evidence is in
`.analysis/scratch/2026-10-19-dbc-enumeration/`; issue#378 remains open.

The DBC enumeration correction passed12native tests plus the formerly failing
TMPDIR=/dev/shm control and strictClippy; committed locally as2bbfc130.
The unchanged old capture remains sealed. At the same remote head, CI then
reported a separate UI/bootstrap defect: job117752240786 completed all preceding
UI source/native suites (including14lifecycle tests) but lacked cargo-clippy in
its minimal1.94.0 installation. Issue#379 records the failure; UIbootstrap now
requests Clippy and a new cross-job native contract requires installation before
first Clippy use. That correction's native contract and committedCI are pending.

The SDK receiver eight tests and existing52CI/disk assertions pass with the
new LLVM20 target registered. Independent externalSDK field comparison now has
three passing tests and strictClippy, preserving101digit integers, lone
surrogates, truncation metadata, booleans and current independentGit tags.
Original SDK event fields come exclusively from the source side of the sealed
CI artifact12146317130; the native candidate supplies none of those expectations.
The actual native SDK controller API remains a recorded missing-function RED;
this is not yet retirement of `check_crash_sdk_transport.py`.

The independent tombstone Worker passed all13tests spanning the full242original
scenario roster plus foundation/configure/tag controls in30.23seconds. Real
30second helper timeout and owned cleanup passed. Its dynamic reference
controller has an actual missing-module RED and is being implemented; controlled
source cancellation and supplementary Unicode-date controls remain pending.
LLVM18 node279's complete six-test target passed, including both prefix planners,
38generic comparisons, actual/19generic terminal boundaries and26ownership
contexts (760.44s, no skipped/filtered tests). LLVM20/prior regressions are pending.

UI/bootstrap follow-up: all40native CI-policy tests plus13disk-policy tests
now pass with the explicit UI Clippy installation. Current native regression
coverage retains all279guards and177baseline argument entries. CloudCI at the
previous2106head has43successful component jobs, two diagnosed job failures
(DBC fixture enumeration/UImissingClippy), an expected failed aggregate and ARM
still pending. This does not establish greenCI for the new working tree.

The dynamic tombstone controller now passes all242 actual source/candidate
comparisons across seven scopes (3integration tests,40.57s), including a request
roster comparison against independent original captures. Full independent source
13tests, Version controller12tests and Version source12tests also pass in the
same gate. Stale PASS receipts on failed source preflight were reproduced through
the real Version CLI and new tombstone controller, then fixed in both entrypoints;
issue#380 remains open pending committedCI. Six of seven ownership/wire controls
passed, including actual cancellation of source and separate native retrace
process groups. The remaining wire control correctly exposed strictUTF8 stdin
handling where unchanged Python uses surrogateescape; repair/revalidation is
pending. Scoped controller/source Clippy passes after compile/lint repairs.
Evidence: `controller-green2-tests.log`, `controller-green3-ownership.log`, and
`controller-green4-clippy.log` under the Version source-closure scratch directory.

LLVM20 node279's same complete six-test target also passes (747.63s). Existing
adjacent model regression targets and strictClippy remain pending. These CPU
compiler results are not device, whole-runtime or project-wide completion.

Native `check-crash-sdk-transport` now passes the full actual15-envelope run
(14original SDK comparisons plus1native I/O exception), six field/controller/CLI
regression tests and scoped strictClippy. The real CLI under strace performed324
execve attempts with zeroPython attempts and emitted a PASS receipt with current
candidate hash and10independent live source pins. Candidate hash was
`d0ed2b0fa8844134d7ba4539edb421d0a26fb6bf49370aba26ba7a1d12739ad7`.
Source pins are verified before starting HTTP and again before PASS. Both the
pre-close and post-worker-join queues must be empty. CLI arity/preflight failures
cannot publish or retain an old success manifest.

Telemetry CI now builds/runs native tombstone and SDK controllers, requiring
source/controller/field tests, cancellation/wire controls and strictClippy. Three
new512MiB reservations bring the total to282; policy regression execution for this
new workflow is pending. Shared original Python helpers remain available to
unconverted safety/runtime/managed-entry callers; no premature deletion occurs.
Inventory180replaced-local/7partial/3393pending records that distinction.
Evidence: `.analysis/scratch/2026-10-19-sdk-receiver/sdk-runtime-first.log`,
`sdk-runtime-clippy.log` and `native-sdk-controller/{exec.trace,result/manifest.json}`.

The combined current controller gate passes83integration tests (13disk,40CI,
8receiver,3SDK field,3SDK runtime/CLI,3tombstone controller,13source), plus8focused
library ownership/wire/date controls and strictClippy. The stdin surrogateescape
repair now agrees with the independently sealed original three-row transcript;
explicit byte-request decoding remains strict. Unicode-year/date controls pass.
The complete242comparison target ran again in40.71s and source13tests in30.23s.
Current source files match the pre-gate source seal. All282CI disk reservations
and177baseline entries are verified. Logs are `combined-controllers.log`,
`combined-ownership.log`, and `combined-clippy.log` in SDK receiver scratch.

A separate managed-entry false-PASS defect is reproduced from the unchanged
original tool: unknown `--case nonexistent-control` skips all44comparisons and
exits0 with PASS/0results, even with /bin/true as both candidate paths. Issue#381
tracks the native admission correction and valid22scenario preservation. The
independent44original source-side results were retrieved from exact-head CI
artifact12146936832, with the same2106source tree as the telemetry captures.
This is future closure evidence, not a completed managed-entry conversion.

The subsequent16 source-only retrace timeout controls exposed four numeric/error-
phase differences. The independent source port now retains NaN ValueError,
large/infinite timeout OverflowError, integer-division overflow and actual spawn
error precedence using the existing owned runner. All13 source tests pass again;
9focused source/controller ownership/wire/date/numeric tests and strictClippy pass.
The native safety checker separately passes all4tests and all4actual retraces,
preserving three benign source-only shell markers and literal native filenames.
Its original Python CLI remains until native CI migration and post-removal proof;
no removal is claimed here. Evidence: `source-timeout16-first-green-all.log`,
`safety-first-green.log`, `source-safety-owned.log`, `source-safety-clippy2.log`.

### 2026-10-19 native tombstone live/safety retirement and artifact identity

The safety, runtime and shutdown checkers now have native CLI owners and mandatory
telemetry CI commands. All three standalone Python checkers are removed after a
frozen executable comparison before and after deletion. Shared original source
helpers remain for consumers whose conversion is incomplete. Inventory is now
183 replaced-local, seven partially replaced and 3,390 pending out of 3,580.

The new runtime executes both reporting cases, four startup failures, real
cereal/msgq delivery, log files and local HTTP. Shutdown checks preserve the
original SIGINT budget, owned retrace descendants and failure cleanup. Historical
hashes are allowed only for the three retired checker paths; changed or dangling
copies still reject, and every remaining live policy input is required.

Issue #383 was reproduced before repair: modifying the binding after the real
daemon's readiness still produced PASS. Runtime and shutdown now freeze all
candidate/collector/binding/worker hashes before execution and verify them again
before success; emitted identity fields use that frozen map. The mutation test
fails on the prior implementation and passes after repair. Existing success
manifests are invalidated on preflight failures and cancellation.

Private evidence is under `.analysis/scratch/2026-10-19-tombstone-live/`:
`artifact-change-red.log`, `retirement-full-green3.log`, and
`retirement-frozen-qeff2mpd/{snapshot.json,before/proof.json,after/proof.json}`.
Each frozen phase recorded 22 runtime, 22 shutdown and 53 safety exec attempts,
zero Python attempts, using the same seven SHA-verified ELF files and binding.
The final after-deletion CLI durations were 19.05s, 18.20s and 1.17s respectively.
The freeze checks 17,402 compiler-input paths, including dependency aliases.
Only the three explicitly retired Python paths may be absent after deletion;
this is not a claim that the frozen binaries were rebuilt from deleted inputs.

A separate post-removal Cargo rebuild passes 74 affected integration tests:
13 disk guards, 40 Rust CI isolation contracts, six workflow contracts, seven
runtime, five safety and three shutdown tests. CI isolation first rejected the
old feature-name assertions; they now require the new live feature, all native
commands and tests, and prohibit all three retired Python calls. All 284 disk
reservations and 177 inherited entries remain checked. The final test log is
`post-removal-tests2.log`; the preceding scoped strict Clippy passed in 6.11s.
Exact-commit remote validation is still required for this local increment.

### 2026-10-19 node279 final affected compiler validation

The six new biased SiLU-chain tests and 15 affected prior tests pass under each
of independently pinned LLVM18 and LLVM20: 42 executions, zero failed/ignored/
filtered. Strict scoped Clippy also passes. Source, helper, frozen ELF and provider
hashes are recorded in `.analysis/scratch/2026-10-18-batched-matmul/NODE279-FINAL-PROOF.json`.
Observed source HEAD is ecfb7774eea9577d4c3c396213a3ecbd0b7d73c9; older frozen
binaries retain their original build-time metadata. These results establish the
changed compiler admission and prior affected behavior, not complete model
compiler retirement, device operation or measured performance gains.

### 2026-10-19 exact-head remote validation remains open

Rust run39159654100 tests ecfb7774. At the latest observed snapshot, 41 component
jobs passed, four remained active and telemetry failed. The failing Version
startup cancellation test did not observe its child-start marker within 10s.
Issue #382 records this failure. Two focused reruns and all 12 local Version
tests pass, which does not establish the remote cause or repair. Added diagnostic
output retains child stderr and process state on this failure without increasing
the deadline or weakening cancellation assertions. A new exact-head CI run must
pass before integration. Device boot, recording and upload acceptance remains
user-owned under the latest instruction; no device result is claimed here.

Later in the same run, UI/connectivity job117770159756 failed the `strict`
environment case in `ui_render_reference`: expected exit0, actual exit1 and
`Render FPS dropped critically`; eight other tests in that target passed.
Issue #384 tracks diagnosis separately from the previous job-budget issue #377.
The actual FPS admission remains mandatory; no threshold change or blanket retry
is a repair. Fast checks39159649278/39159654112, Integration39159654103 and user
 docs39159654074 pass for ecfb7774. The Rust gate is not successful.

### Final provenance interruption regression (#380, 2026-10-19)

The actual native runtime checker was stopped at its final provenance write with
an owned FIFO, after all scenario and artifact checks. A pidfd-targeted SIGINT
was delivered before the FIFO reader released that write. The checker exited
unsuccessfully but still left a `result: PASS` manifest: the focused regression
failed in18.68s. Private receipt: `2026-10-19-tombstone-live/final-cancel-red.json`.
The repair checks cancellation immediately before the aggregate write and removes
that aggregate on the final interrupted return in both runtime and shutdown
controllers. Verification of this repair is recorded separately below; this
reproduction is not a device result.

The repaired actual runtime suite passes8/8 in18.79s, including the same final
FIFO/SIGINT regression; the shutdown suite passes3/3 in17.51s. Scoped strict
Clippy passes in5.86s. All were rebuilt from the current source with the mandatory
real daemon/collector/Params/worker artifacts. Raw local log:
`2026-10-19-tombstone-live/final-green.log`. Together with unchanged policy/safety
suites from the prior gate, the live-retirement validation now covers75 tests.

### Version final-write cancellation follow-up (#380)

The same late-cancellation window remained in `check-version-reference` after
its preflight invalidation repair. A new actual-CLI regression completes all171
comparisons, blocks the final manifest open on an owned FIFO, sends pidfd SIGINT,
and releases/drains that exact writer. The old checker returned failure but
left the manifest visible (RED9.75s). No timing threshold or source comparator
was changed. The repair checks cancellation before publication and invalidates
the manifest on the final interrupted return. Private raw evidence:
`2026-10-19-managed-entry/version-final-{red,green}.log`; GREEN is recorded only
after inspecting the new result.

The focused Version final-write regression now passes in9.88s after rebuilding
its actual checker. It still observes the complete171-comparison PASS document
at the controlled write boundary, a nonzero interrupted exit and no remaining
manifest path. The full Version suite and exact-head CI remain distinct gates.

### Strict UI FPS boundary and managed-entry continuation

UI issue #384 is repaired locally in `8f4b556a`. The QA helper accepts an
explicit acknowledged FPS sample for the strict branch; its default still uses
real GetFPS, and the original target20 / critical-below10 behavior is unchanged.
Independent unchanged original AST and full pyray executions establish 9 failure
and10 success. All14 prior environment cases retain pixel/state/recorder checks;
the added actual9-FPS CLI case requires exit1, no final PNG/state, and recorder
EOF. The focused gate passes in62.80s and strict Clippy/C++ warning checks pass.
This controlled measurement input is not evidence of host/device performance.
The issue remains open for exact-head CI.

Managed-entry remains in progress. The independent source now passes26 original
controls, including invalid/empty/unregistered DongleId and context-acknowledgement
SIGINT. The original wrapper's early-SIGINT `returned` report is retained as its
observed wrapper behavior; the native example preserves its actual
`Outcome::Interrupted`, not a request-derived fabricated outcome. The actual
example previously remained blocked for5.01s; the corrected acknowledgement
loop returns interruption with no body execution, passing in0.02s. This is #385's
QA-protocol defect, not a demonstrated production driving-daemon defect.
Source26/retirement guards and strict Clippy pass; full44 current collector
comparisons and actual Python-helper deletion are separate pending gates.

### Remaining aggregate receipt cancellation gaps (#380)

Actual CLI failures reproduced three Version collector preflight paths retaining
an older PASS: invalid source, missing adjacent worker, and an already-existing
collector directory. The tombstone242-comparison and Version collector final
writes also retained PASS after pidfd SIGINT at a controlled FIFO. The collector
still delivered exactly one logMessage, one errorLogMessage and one disk record;
the regression is receipt publication, not dropped telemetry.

`4bb011cf` invalidates these results before preflight and on failed/interrupted
publication. All4 tombstone-reference and6 collector tests pass (81.23s/3.86s),
with strict Clippy5.47s. Final review latched the tombstone cancellation decision
once so cleanup and the interrupted return cannot disagree across two reads;
the same actual242-comparison final-write regression passes again in40.57s.
Raw RED/GREEN logs are under `2026-10-19-aggregate-pass/`. The separate Version
reference suite passes13 tests plus59 CI policy/disk tests; these are host results.

### Endpoint identity review blocks the managed retirement gate (#386)

An actual managed-entry cancellation test replaced the acknowledged collector's
socket pathname before worker teardown. The outer checker first observed the
path only after teardown, adopted the foreign inode and removed it. The existing
unit check covered replacement after observation and missed this order. Issue386
tracks the reproduced11.94ms cancellation and both distinct inode identities.
No managed Python files have been deleted. The first frozen bundle is explicitly
invalidated because a later source correction occurred after its compilation;
no before/after proof used it. A fresh source-matched build and proof must follow
the endpoint ownership repair. Version collector and tombstone runtime use the
same late-observation pattern and are included in the focused audit.

### Managed-runtime checker retirement and endpoint ownership (#385/#386)

The seven managed daemons and collector were freshly built from current sources.
The unchanged Python checker passed all14 daemon/context cases; the Rust checker
matched the exact original scenario rows before and after removing
`rust/tools/check_managed_runtime.py`. Both runs used the same nine sealed ELFs.
The post-removal trace covered execve and execveat:44 attempts, zero Python/pypy
executions. Ten integration tests and one socket-identity unit passed; strict
Clippy passed. Earlier cached-daemon observations remain separately labelled.
This is host checker evidence, not device boot/recording/upload acceptance.

Issue#386 was reproduced independently in managed-entry, Version collector and
tombstone runtime controllers: observing the endpoint only after worker teardown
could adopt and unlink a foreign replacement. A private invocation-bound channel
now transfers the live collector identity before candidate launch. The owner
holds an O_PATH descriptor and removes only the matching socket identity.
Managed ownership/readiness contracts passed23 tests; Version/tombstone helper2,
focused2, runtime9, shutdown3, collector7 and positive collector1 passed with
both feature combinations clean under strict Clippy. Abrupt death before the
authenticated acknowledgement can conservatively leave an owned socket; this
window is explicit and is not described as complete IPC cleanup.

Private proof records: `2026-10-19-managed-runtime-tools/FINAL-PROOF.json`,
`2026-10-19-managed-entry/ENDPOINT-386-FINAL-HANDOFF.json`, and
`2026-10-19-endpoint-owner-386/FINAL-EVIDENCE.txt` under `.analysis/scratch/`.
Exact-commit CI and the subsequent managed-entry retirement proof remain separate
gates.

### Managed-entry three-helper removal proof (2026-10-19)

A fresh build froze check-managed-entry, managed_child, logmessaged and the Params
binding before removing the three managed-entry Python tools. The dep-info input
snapshot contains17,405 project paths (including broad logging provenance inputs,
not17,405 compiled Rust modules). Current44 source/native comparisons and the
original subscriber-before-publisher readiness scenario passed before deletion
(257.65s/5.07s) and after deletion (260.93s/5.38s). Both phases traced265 matrix
plus4 readiness execve/execveat attempts; no Python/pypy execution occurred.
The same ELF/binding hashes and every retained snapshot input matched throughout;
only the exact three retired paths were admitted absent. The older first freeze
was explicitly invalidated before it ran any retirement proof and is not reused.

Current private bundle: `2026-10-19-managed-entry/retirement-frozen-sdtt5wia`.
Its before/after proof files bind snapshot SHA256
`f6a90c0c8b976dd28f3a80495105af7771637a5f418c9451a48f9f6f42bda0ab`.
Parent CI-policy gates60, Version13, managed collector/namespace units4 and shared
endpoint units2 passed with strict scoped Clippy. An initial broad all-lib run
passed96 and failed the unrelated retrace test due to a missing required probe;
startup CI now explicitly selects the changed managed and endpoint units, while
retaining the existing retrace gate in its proper prerequisite-bearing job.
Fresh post-removal build/integration results and the managed-runtime#386/#387
follow-up remain separately required.

### Managed-runtime review repairs (#386/#387)

The independent review reproduced filesystem inode recycling after collector
closure and actual FIFO executable prehash hangs. The old permanent socket unit
and both daemon/collector FIFO regressions failed before repair. Namespace now
retains an O_PATH descriptor through cleanup; executable provenance opens
nonblocking, checks the descriptor is a regular file, then hashes it. Special
files still fail at the original spawn/error phase, preserving artifact behavior.

The repaired checker passed12 integration tests (25.47s),2 ownership units
(0.11s) and strict Clippy (5.98s). A fresh repaired frozen checker also passed
all14 post-deletion cases against the unchanged original receipt and same eight
daemon/collector ELFs:44 execve/execveat attempts, zero Python. This later proof
binds checker SHA256
`27efb65adc2c9793ab4f6e9a81a8ff2ee8e710c86b6feb2626ebd87b18315a2d`;
it is distinct from the earlier same-checker before/after proof and does not
relabel that older executable as containing these repairs. The exact-source CI
remains pending. Details: `2026-10-19-managed-runtime-tools/REVIEW-REPAIR-FINAL-HANDOFF.json`.

The post-deletion managed-entry rebuild completed successfully, rebuilding the
logging/Params/version/crash/managed-entry/collector dependency chain. All eight
focused integration targets passed21 tests, including the complete44-comparison
CLI (252.91s), original readiness, source admission, ownership and SIGINT gates.
Strict scoped Clippy, all60 CI policy/isolation/disk checks, full workspace
rustfmt check and git diff check passed. Log:
`2026-10-19-managed-entry/post-retirement-gate.log`. This is a fresh post-removal
build gate in addition to, not a relabelling of, the earlier same-ELF proofs.

### Version test admission repair (#382, local follow-up)

A deterministic real-process regression confirmed that rejecting the second PID
during startup admission dropped an already-armed first owner and killed its Git
process before acknowledgement. The test now retains unarmed pidfds until both
unique-token/live-identity checks pass, then arms both cleanup owners together.
All14 Version tests passed under default parallelism (9.80s), with strict Clippy
and formatting clean. The10s startup and2s cancellation limits and production
code are unchanged. This repairs a proven harness bug; it does not establish that
the earlier CI failure took that path. The exact committed telemetry rerun is
still required. Evidence: `2026-10-19-version-382-review/FINAL-EVIDENCE.txt`.

The captured empty process-title TSV row retains its significant trailing empty
fields. A single-path .gitattributes whitespace exception preserves those original
bytes; the fixture itself was not edited. Staged diff checking passes.

### Registration clock checker retirement and independent LLVM20 CI (#390)

`check-registration-clock` replaces the original standard-library Python
controller. Its independent Gregorian policy preserves all13 SystemTime cases
and72 additional original calendar controls. Actual CLI, runner, raw-path and
failure captures preserve report bytes and failure phases. Review reproduced
executable/source rebinding and stale-success receipts before the repair; the
native controller now rejects observed input drift, removes only stale successful
reports and preserves original mismatch/failure artifacts. Owned cancellation
and a60s child bound are explicit additions to the originally unbounded child.

The repaired checker SHA256
`f655d2b6b9bea2719ce7ec8e2d48dc55eede3cf17459e2518e7e94265a862f95`
and probe `77ab0e6745cb64af10d56ff09f3a9a530fe766b7888a41fa8d7fea67af093728`
passed the same full original13-case report, CLI/failure/raw-path controls and
binding negatives before and after deleting `check_registration_clock.py`.
Each full execve/execveat trace has two native execution attempts and no Python.
The historical source text is nonexecutable source-guard fixture data, never an
execution fallback. These are bounded identity checks, not an atomic snapshot.
Fresh post-deletion16 tests, strict Clippy and a rebuilt CLI also pass.
Evidence: `2026-10-19-registration-clock-tools/FINAL-PROOF.json` and
`post-delete-fresh-{tests,build}.log`.

Old run39159654100 spent85m02s in workspace tests and then over40 minutes in
serial LLVM20 replay, leaving later mandatory source/package checks waiting.
The unchanged LLVM20 provider staging and all19 graph plus10 primitive targets
now run in a separate90-minute required job. The moved three-step YAML is
byte-identical (SHA256 `c93a605554df4cab215c2897a0b884559bafea9abec7478025873215fa2fc0e7`).
LLVM18 workspace tests, original comparisons and release builds remain intact.
The new job has its own guarded toolchain/model preparation and must succeed
for `rust checks` to pass. A regression first failed on the missing independent
job; all62 native CI/disk/policy tests now pass, including missing-dependency and
wrong-result negative controls. All177 inherited guard arguments remain exact;
299 total guards cover35 jobs. Exact remote CI and timing improvement remain
unverified until the new committed run. Evidence: `2026-10-19-llvm-ci-split/`.

### Upload completion identity and changed-source fixes (#388 / #389)

Actual native REDs reproduced marking an untransmitted replacement file and
publishing a mixed same-length folder body. A further callback RED caught a
same-inode rewrite between accepted-response logging and marking. The uploader
now retains raw/compressed body descriptors through inode-bound `fsetxattr` and
rechecks after logging. Folder upload rejects observed changed-source metadata
before terminal HTTP EOF. Existing status/size policies and no-read-permission
marking remain covered; this is not an atomic content snapshot guarantee.

All47 affected package tests, strict all-target Clippy, formatting and fresh
example builds pass. Unchanged-original comparisons also pass212 filesystem,
10,000 backoff,32 uploader HTTP and99 folder/WebDAV cases. Exact source/frozen
ELF seals and permission/event-ordering limits are recorded in
`2026-10-19-upload-ownership-audit/FINAL-HANDOFF.json` and the module audit.
This does not retire their still-live Python comparison controllers or establish
actual NAS/device upload acceptance. Exact committed-SHA CI remains outstanding.

The old `ecfb7774` workspace job finally ended cancelled at22:12:58UTC after its
150-minute budget; LLVM20 tests were still advancing when cancelled. Later
source/package stages were not run. The run is a failure overall, also retaining
the earlier telemetry#382 and UI#384 failures. ARM and other completed component
jobs passed. [Exact old run](https://github.com/bin9208/openpilot-rust/actions/runs/39159654100).
The new LLVM20 job separation in `c7ea57e0` still requires its own remote run.

### Four shared-helper retirements: Version, tombstone and SDK (2026-10-19)

The last executable consumers of `check_version_reference.py`,
`check_tombstoned_reference.py`, `tombstoned_reference.py` and
`check_crash_sdk_transport.py` now use the independent native source/collector
implementations. Their exact original identities are kept as historical hashes
only when the files are actually absent. Changed, dangling, directory and missing
mandatory runtime-source controls remain failures. Native Version, tombstone,
SDK, managed-entry and readiness reports distinguish live inputs from retired
historical inputs; admission and final checks detect persistent roster drift.

The SDK final-manifest cancellation case reproduced a false success receipt before
the repair. It now latches the result/cancellation decision and invalidates that
receipt on either failure or interruption, continuing #380's bounded cleanup work.
109 affected tests, strict scoped Clippy and formatting passed before removal.

A fresh build supplied15 frozen ELF files with17,513 recorded project dependency
paths (including logging build-provenance inputs). Snapshot
`fc37723acad19f78c98b16ef0b4f6a4f1d10e620b890043b835f3d3d110ddf16`
was verified before and after deleting exactly the four sealed helpers. Both
runs passed all nine tools: Version171 comparisons, live Version collector,
tombstone242 comparisons, SDK14 events plus1 native IO exception, tombstone
runtime/shutdown/safety, managed44 scenarios and subscriber-before-publisher
readiness. Each phase made1,444 traced execve/execveat attempts and zero Python
attempts. Only Version/SDK had an explicit Git-only search path; other tools used
an unavailable PATH. Private proof orchestration is outside these traced process
trees. The Params binding identity remains evidence data, not an executed Python
extension in these native checks.

The before/after bundles preserve the same executable hashes; this establishes
removal independence, not a post-removal rebuild. Fresh post-removal validation
and exact committed-head CI are recorded separately when complete. Current
inventory:192 replaced-local,3 partially-replaced and3,385 pending baseline
sources. Repository-wide conversion and user device acceptance remain open.

Fresh post-removal validation completed: rebuilt native artifacts,86 affected
unit/integration tests, two actual managed-entry comparisons, strict scoped
Clippy and formatting pass. The complete44 managed scenarios were already run
with the identical frozen executables on both sides of deletion; they were not
repeated a third time. A post-removal Version test setup initially failed because
its copied fixture no longer created the retired helper directory; creating that
directory fixed the setup without changing assertions. All11 production source
hashes still match the frozen before/after proof. Exact committed-head CI remains
pending. Receipt: `2026-10-19-partial-helper-closure/postdelete-final-proof.json`.

### Logger queued-audio shutdown (#391)

A one-line complete-frame drain before existing tail padding/EOF repairs the
native reproduced1024/1025/3585-sample loss;1023 remains a passing control.
All16 loggerd package tests, strict Clippy and a fresh daemon build pass.
Fresh unchanged C++ originals match ordinary audio, queued-audio and flush
scenarios. The local FFmpeg capability failure was resolved with isolated
Ubuntu6.1.1 validation tools, preserving all comparison assertions. See the
module audit and `2026-10-19-logger-audio-391/FINAL-HANDOFF.json`.
This does not retire the live Python logger comparison controllers or establish
device recording acceptance.

### Exact-head CI follow-up: LLVM20 required-job contract (#390)

Published `74d211ba` runs [Rust port39174296648](https://github.com/bin9208/openpilot-rust/actions/runs/39174296648),
[Integration39174296715](https://github.com/bin9208/openpilot-rust/actions/runs/39174296715)
and [Fast39174296592](https://github.com/bin9208/openpilot-rust/actions/runs/39174296592).
The Card/CAN job117813668223 failed its inherited exact required-job set because
`model-llvm20` had been added to the aggregate but not that test. Its preceding
full Hyundai checks passed1,272 CarParams scenarios,35,520 CarState/actuator ticks
and91,316 exact CAN frames. This does not clear unrelated Card/NAS acceptance.

The same required-set assertion reproduced locally. Adding the new required job
to its expected set preserves exact equality and all fixture/full-schema checks;
all39 inherited Card CI/admission/fence tests then pass. No workflow dependency
was removed. The first local invocation lacked the test tools PYTHONPATH and did
not collect; the corrected source-native setup produced the actual RED/GREEN.
Private evidence: `2026-10-19-llvm-ci-split/card-aggregate-*` and
`2026-10-19-publish-batch/card-failure.log`. Other jobs remain separate pending
checks, and the published run is not an overall success.

### Native Bluetooth input decoder checker (2026-10-19)

Only `rust/tools/check_bluetooth_input_decode.py` is retired. The native
`check-bluetooth-input-decode` independently reproduces the unchanged source's
25 extrema,10,000 seeded records and24 incomplete buffers, preserving signed
fields and exact timestamp binary64 bits. The sealed synthetic source fixture
and14 original child/error/artifact boundaries remain permanent tests. Actual
REDs exposed Boolean timestamp coercion and executable-argument retargeting;
the repairs retain numeric equality and reject mixed executable/source identity.
Leader-only cancellation after acknowledging child/descendant pidfds proves
owned cleanup within two seconds. The original30-second child deadline remains.
The source ABI is explicitly Linux little-endian LP64 input_event(size24).

The unchanged original checker and fresh actual `input_fixture` passed all10,049
records. All five input/native/source/result artifacts are byte-identical to the
native run. Fresh frozen bundle `frozen-a0v56j_9` then passed before and after
parent deletion using the same two ELFs: checker SHA256
`65871219fc614fb48d7e9f37717461b84c65680df1bd1287cfa80a6a72e90cc3`,
fixture SHA256
`36cd76ce199ea08ccc1fd7f2caa113fdf9ff813b0f4a1205fbb7c57d3a12792d`.
The17,443-input source receipt SHA256 is
`06557fc06d99c1e05d5164328ca87e8926848954b7652f7a4b4183e3cac64e9b`.
Each phase passed six CLI controls and empty-PATH execution traces with zero
Python child execution. An after-proof harness correction recognizes only the
resolved alias of the one removed helper, requires its original SHA256
`2396e77d952213074555f47413934d090500d7d1ec8a338f06c38adefda6cc5a`
and actual absence; source/ELF seals were unchanged. Failed harness receipts
remain private; they are not reported as successful proofs.

Fresh post-deletion builds, eight permanent Bluetooth tests,43 CI-policy plus13
disk tests, scoped strict Clippy and formatting pass. The freshly built checker
again passes the10,049 actual cases, all five artifact bytes and empty-PATH
zero-Python tracing. Mandatory UI/connectivity CI explicitly builds this feature,
tests its target with a fresh fixture and runs strict Clippy. The caller's
explicit/default native path preserves the full14-case roster; the other13
Bluetooth Python cases remain pending. Three new growth guards bring the Rust
workflow count to302 while retaining the inherited177-guard hash.

Private evidence: `2026-10-19-bluetooth-decode-tools/` source captures,
`frozen-a0v56j_9/{before,after}/proof.json`, `postdelete-gate.log` and
`postdelete-actual-proof.json`. Inventory recount:193 replaced-local,3 partial,
3,384 pending out of3,580. This is one local tool retirement; exact committed-head
CI, full repository conversion and user-owned device validation remain open.

### Native statistics producer controller and source helper (2026-10-19)

Retired `check_stats_producer.py` and `stats_source_producer.py`. The unchanged
original first passed all eight original/native scenarios over real ZMQ. The
sealed original-only fixture SHA256 is
`9984d69f1112d59cbcecd5af38aeb9c4e95a19ca602f0f88a546e750adfa654e`.
The independent Rust checker preserves fork reconnect/drop ordering, all1,000
pressure messages after20,000 sends and absence of extra messages, and ten
Cereal-typed values with exact floating-point wire spelling. Its result clearly
labels the captured original reference; it does not pretend to rerun Python.

An actual missing-API RED was followed by a real cancellation failure: killing
the worker left its temporary IPC directory behind. Parent ownership of that
directory fixes cleanup without changing transport assertions. Six permanent
checks now cover real transport, independent expected values, existing-output
ownership, missing executable rejection, acknowledged-child cancellation and
source mutation rejection before any worker. Held nonblocking regular-file
handles verify source/executable identities before and after execution. Only
the exact two retired helper paths may be absent from the eight original pins.

Frozen bundle `frozen-e00U4H` passed before and after deletion using the same
four ELFs. All nine artifacts are byte-identical; each phase traces nine execs
and zero Python execution with an empty PATH. Checker SHA256 is
`b8901bf409ff59d37cf637f10b7c2756540b04575ed61d08dba33d41efe53c52`.
The fixture and production source seals stayed unchanged during those proofs.
A sixth source-mutation test and final documentation were added afterwards;
those additions are covered by the fresh post-deletion gate rather than claimed
to be in the historical source seal.

Fresh post-deletion example/checker builds pass six regression tests,43 CI
policy tests,13 disk-policy tests, strict Clippy and scoped formatting. A fresh
actual four-scenario run again passes with nine execs and zero Python; all four
ELF hashes match the frozen bundle. Mandatory telemetry CI now builds/tests this
feature, runs strict Clippy and calls the native command. Three new growth
guards bring the workflow count to305, preserving the inherited177 guards.
Independent source review found no remaining blocker in this bounded closure.

Evidence: `2026-10-19-stats-producer-tools/{api-red.log,native-first.log,`
`native-green.log,frozen-e00U4H/,postdelete/}`. Inventory recount:195 replaced-local,
3 partial and3,382 pending out of3,580. Other statistics Python consumers and the
original statsd source remain explicitly pending; exact committed-head CI and
user-owned device startup/recording/upload verification are separate gates.

### Native continuous statistics controllers (2026-10-19)

Retired only `check_stats_daemon.py`, `check_stats_failures.py` and
`check_stats_log_fault.py`. The shared `stats_source.py` remains: the mandatory
`check_stats_interrupted.py` still consumes it. Original `statsd.py` remains too.
The unchanged source/native controllers first passed all ten scenarios. Their
source-only result capture SHA256 is
`6376c0a5e990c4040ffa115cde2fd357810c760a32a85dc2ba9f49c4eb2a57c1`;
all680 Unicode15.0 decimal inputs are separately sealed. The native controllers
exercise real examples, Params files, msgq/ZMQ and the independent collector,
then compare exact results with the captured fixed-input original scenarios.
This is not a general replacement for the original statsd source.

The API RED preceded implementation. Eight permanent checks cover the ten real
scenarios, malformed clock output, missing candidates, foreign receipt retention,
source/candidate/binding FIFOs, three live source identity changes, leader-only
cancellation and a replacement collector endpoint. Independent review reproduced
a cleanup bug: after a foreign listener replaced the live collector socket,
cancellation deleted the foreign listener. Authenticated live endpoint identity,
a held inode and parent ownership now preserve replacements while cleaning the
owned endpoint. Candidate/collector processes retain one outer process group;
private IPC directories survive worker death only until parent cleanup. The
conservative pre-acknowledgement socket window remains an explicit limitation of
the existing endpoint-owner helper; no endpoint observed only after teardown is
adopted. A DEBUG transport ACK drains startup traffic before each scenario and
does not create a disk log record.

Frozen bundle `frozen-3YlUWrRh` passed all three CLIs before and after deletion:
ten scenarios,34 traced execs and zero Python execution with an empty PATH per
phase. All five ELFs stayed identical and normalized source/native comparison
artifacts were byte-identical. Checker SHA256:
`6f741f6b96a845165e61fb2285725bc20ca43367b1fbe4551dcb42f5366ad016`.
Fresh post-deletion builds passed eight tests (including all ten live scenarios),
44 CI-policy tests,13 disk-policy tests, strict Clippy and scoped formatting;
all five rebuilt ELFs matched the frozen copies. The first post-deletion format
check caught one test-only layout issue; the corrected check passed. Historical
failure logs are retained separately.

All three mandatory telemetry callers now build/test/run the native tools.
Three additional512MiB guards preserve all inherited177 guards. At this unit's
commit the total is308; the concurrently prepared but uncommitted procLog unit
adds three more, explaining311 in the combined working-tree validation.
Evidence: `2026-10-19-stats-daemon-tools/{capture.sh,api-red.log,`
`independent-review.txt,review-endpoint-y480nbt7/,lifecycle-tests.log,`
`frozen-3YlUWrRh/,postdelete-*.log,postdelete-status}`. The final test-only format
change follows the historical scoped source seal and is covered by its fresh
format check, not retroactively included in that seal.

Inventory:198 replaced-local,3 partial,3,379 pending out of3,580. Exact committed
head CI, complete repository conversion, dev integration and user-owned device
startup/recording/upload acceptance remain separate and unfinished.

### Native sound policy and fixture generation (2026-10-19)

Retired only `rust/tools/check_soundd.py`. Independent Rust policy code reads
actual generated WAV bytes and produces all141 requests without golden lookup.
Six original schedules cover846 states and6,348 samples; the raw legacy results
remain distinct from the intended #392 finite-clip correction. Original asset
bytes,17 CLI cases, source pins and full state/sample captures are retained.
The remaining seven-scenario Python daemon checker now calls the native
`create-soundd-fixture-assets` executable; that checker is not retired.

The new API tests failed before implementation. Sixteen permanent tests cover
reference policy, actual WAV generation, generic inputs, controller failures,
retired-source reintroduction, real retained-daemon argparse and mandatory CI
routing. The actual CLI test caught a missing helper argument declaration;
the parser was repaired and its regression now passes. The optional prebuilt
PortAudio fixture path permits same-artifact validation while CI retains its
normal C fixture build. No physical sound output is claimed.

Frozen bundle `frozen-final-u62azm6w` passed six policy and seven actual daemon
CLI scenarios before and after deletion using the same five artifacts. All24
policy JSON/stderr files are byte-identical; normalization is limited to four
private generated asset paths. Each policy phase traced seven execs and zero
Python. The explicitly retained daemon checker used one Python process in its
15 execs; it preserves all seven runtime assertions. Policy checker SHA256:
`51304bef1123514e5d9c8c77cc02ce67d6f4e3d53365ee0d0447095709ab3363`.

Fresh post-deletion builds, all16 sound tests, scoped strict Clippy and formatting
pass. All four rebuilt executable hashes match the frozen artifacts. Shared
policy validation passed13 disk tests and43 existing isolation tests; an added,
uncommitted ARM policy test failed because it incorrectly forbade the inherited
ARM branch condition. That separate assertion is being repaired in the ARM unit;
this evidence does not describe the combined working-tree suite as all passing.
The sound-only commit retains all177 inherited disk guards and adds three512MiB
guards, bringing its total to311. Evidence is under
`2026-10-19-soundd-policy-tools/{RETIREMENT-EVIDENCE.json,postdelete-*.log,`
`actual-final-before-lnoahvxl/,actual-final-after-om9d2jp7/}`.

Inventory:199 replaced-local,3 partially replaced and3,378 pending of3,580.
Exact committed-head CI, repository-wide conversion, integration and user-owned
normal boot/recording/upload acceptance remain unfinished.

### Native complete procLog and AArch64 validation (2026-10-19)

Retired exactly `check_proclog_reference.py` and `check_aarch64.py`. Independent
original-only captures contain176 complete Events across two smaps modes,
22 cycles and four clock/page-size pairs. Full-schema reflection compares every
field; mutations cover all70 scalar/text leaves. Another88 original wire frames
and28 original protocol controls preserve error ordering, truncation, cache
cadence, queue integer parsing and original trailing-frame acceptance.

The ARM controller independently inspects ELF architecture and absence of an
interpreter/shared dependencies, retaining the original readelf predicates.
Twenty original controls include six actual readelf cases. Actual native IPC
checks use the static probe through QEMU and preserve all original report fields,
self-test output and the three-frame assertions. Fake-runner process tests are
labelled lifecycle checks and are separate from this real QEMU execution.
Source/executable/runner identities, FIFOs, raw paths, leader-only cancellation,
foreign directory replacements and existing report/source-alias preservation
are covered. Original failures preserve earlier report bytes; the native tool
uses atomic replacement and removes only its owned result on cancellation.

The original cached musl binary failed PSS/nice assertions and was not accepted.
A fresh isolated source-matched musl build passed the unchanged original and
native checkers. Frozen bundle `frozen-40igxjin` passed the combined44 procLog
cycles and three QEMU frames before and after removal with identical artifacts,
an empty PATH and zero Python executions. Fresh post-deletion builds repeated
the actual44+3 comparisons with11 execs and zero Python. Both checker hashes,
the trace, fresh static probe and runner remain identical to the frozen bundle.
The static probe SHA256 is
`0c92e17f00c9c2496d1a1e44b98bfe54381a3239d44755e7309fb76195573acb`.

Post-deletion validation passes77 tests (11 ARM,9 procLog,44 CI policy,13 disk),
both strict Clippy scopes and formatting. A post-deletion test initially read a
retired helper as its source-alias target; changing that fixture to the retained
source-pinned `device_probe.py` preserves the original alias assertion. The ARM
CI assertion also now checks its inherited branch condition exactly rather than
incorrectly forbidding it. All inherited workspace example builds and static
workspace/FFmpeg builds remain; the new host checker build is package-bounded.
Seven new disk guards bring this unit's commit to318 while preserving the
inherited177 guards. The subsequently prepared statistics route adds three more
only in the working tree. Existing daemon/device Python consumers remain.

Evidence: `2026-10-19-aarch64-check-tools/{FINAL-HANDOFF.txt,`
`fresh-postdelete-proof.json,postdelete2-*.log,frozen-40igxjin/}`. Inventory:
201 replaced-local,3 partial,3,376 pending of3,580. QEMU remains host validation;
`device_validation` is `not_run`. Exact committed-head CI, complete conversion,
integration and user-owned boot/recording/upload acceptance are not complete.


### Interrupted statistics receive closure (2026-10-19)

Retired exactly `check_stats_interrupted.py` and its last shared helper
`stats_source.py`. Fresh original-only fault injection captured five cases:
one/repeated EINTR, EINTR followed by EAGAIN, EINVAL and ENOTSOCK. The independent
fixture SHA256 is30af65083d9293acd0791f11d2ce40cb8eb611d2a7b4211dc82601d91e69020a.
Native comparison uses the real link-wrapped daemon, retained metric flush and
fatal no-file effects, exact trace errors/DONTWAIT, original5-second bounds,
actual Params/msgq collectors and owned process cleanup. The original preload
and binding are provenance inputs; native execution does not invoke Python.
Original `statsd.py` and other source dependencies remain mandatory.

API RED preceded implementation. Independent review reproduced a late-SIGINT
bug after all five workers completed while final stdout was blocked: the old
controller returned success and retained a success receipt. The final write is
now fallible and checked before atomic no-clobber receipt publication, with
owned-inode cleanup on cancellation. A real pipe/PIDFD regression passes.
Original argparse help exit0, invalid usage exit2 and all `--before` positions
are preserved. The historical broken-daemon `--before` comparison contract is
tested separately; no old broken executable was run in this gate.

Frozen bundle `frozen-EwY6f6FE` passed all four statistics CLIs before and after
removing both files:15 cases and50 exec attempts per phase,0 Python with empty
PATH. The same8 artifacts were used;4 normalized reports are byte-identical.
Eleven input maps changed only by removal of the two historical source keys.
The interrupted checker SHA256 is
32f9f970a74495894c0b81fcc85279f00eb62d006337faf78c93cc8827dd5c65.
Fresh post-deletion build,72 tests (7 new,8 predecessor,44 CI,13 disk), scoped
formatting and strict Clippy pass. The four freshly built checkers, producer and
preload match the frozen bytes. Plain/wrapped daemon builds differ solely in
embedded logging source commit0b5c20ca→18685227 and GNU build-id; normalizing
those fields gives complete byte equality. Fresh actual daemon cases passed.

CI preserves the original C boundary/wrapper and routes the full five-scenario
comparison through the native CLI with mandatory tests/Clippy. Disk guards
increase318→321, retaining the original177 baseline. Private proof is indexed by
`.analysis/scratch/2026-10-19-stats-interrupted-tools/FINAL-HANDOFF.json` and
`postdelete-proof.json`. Inventory:203 replaced-local,3 partial,3,374 pending
of3,580. Device/drive validation was not run; real-clock and flood checkers are
separate pending work.

### Native registration and navigation configuration closure (2026-10-19)

Removed `check_registration.py`, `registration_source.py`, `check_navd_config.py`
and `navd_config_source.py` after migrating their callers to native controllers
and independent source references. The four-file closure preserves actual Params,
RSA/EC keys and signatures, HTTP/collector effects and timing, including all 131
registration policies, three real timeout cases and 26 navigation configurations.
Generic input, codec, malformed response, GPS error-order and CLI controls remain
separate from the named matrices. The old binding arguments are removed; shared
original logging/Params and other navigation helpers remain explicitly pending.

The original API and missing-helper guard regressions failed before correction.
A closed-stdout navigation run also reproduced a stale PASS receipt after its
output panic. The native controller now checks and flushes output before creating
an exclusive aggregate receipt, and rolls back only the inode it owns. This
intentionally strengthens the original receipt behavior. Per-case evidence and
failed-comparison reports remain available. Only the exact four retired names may
be absent; present drift, dangling aliases and missing retained inputs still fail.
Historical source hashes are separate from live provenance. No renamed Python
implementation is embedded or reconstructed by maintained tests.

Frozen bundle `combined-gate-TtRrgPUI` passed seven focused units, 27 integration
tests and strict Clippy. Before and after deletion, the same nine executables pass
all 27 tests, registration 134, navigation 26 and 17 actual CLI controls. Independent
original normalizers verify all 268 registration observations and 52 navigation
observations, including signatures and complete Params state. Registration's
aggregate results are byte-identical across deletion; only the exact removed live
source keys disappear. Each phase records 940 registration and 53 navigation exec
attempts with no Python/PyPy execution in the native process trees. Private audit
scripts run outside those trees and are not a maintained runtime fallback.

Evidence indexes: `2026-10-19-navd-config-tools/COMBINED-BEFORE-EVIDENCE.json` and
`COMBINED-AFTER-EVIDENCE.json` (SHA256
`d729d99da83d683400e31d476ccf6cf7c41f8ab29312731b3fea15ece4ca1789`).
Fresh post-deletion compilation is tracked separately below. Current inventory:
207 replaced-local, 3 partial and 3,370 pending baseline sources. CI routing now
passes 46 policy and 13 disk tests, retains 336 growth guards, and requires the
three navigation receipt units as well as the integration targets. This is not
complete repository retirement or device acceptance.

The registration/navigation post-deletion compiler phase finished before the
user changed the validation workflow. Fresh `fresh-post-delete-dgpDDf8O` builds,
seven units and Clippy passed; all nine executable hashes are identical to
`combined-gate-TtRrgPUI`. Its 27 integration tests were not repeated locally;
the existing same-artifact proof and required GitHub Actions remain distinct.

### Native production statistics runtime/flood controllers (2026-10-19)

Removed `check_stats_runtime.py` and `check_stats_flood.py`. The native commands
preserve the real production entrypoint, timestamp bounds, one 0600 output file,
normal SIGTERM and busy SIGINT/SIGTERM behavior. The busy cases require two live
producers, at least 10 user CPU ticks, no shutdown flush and the original 2s wait;
startup and ordinary shutdown retain 10s and 5s bounds. Independent original runs
covered three cases and 16 CLI/effect controls before implementation.

Missing API and exact-two-source absence regressions failed before correction.
Nine new and 15 predecessor tests, formatting and strict Clippy passed before the
workflow change. Two frozen checkers and two matched production artifacts pass
all three cases before and after deletion, with 15 exec attempts and zero Python
in each native tree. Stable reports match the original results; four provenance
maps lose exactly the two removed paths, and retained hashes remain unchanged.
Actual clock values, scheduler-dependent started state, CPU ticks and elapsed
waits are validated and retained rather than compared as fixed expected numbers.

A predecessor cancellation test also encountered transient `/proc/environ`
visibility around exec. It now retains a pidfd, requires the same private token
and a live process within the existing 15s startup bound, then performs the same
2s shutdown and endpoint ownership assertions. This is part of the #396 test
repair; no additional daemon defect is inferred.

The root receipt uses no-clobber publication and inode-checked cleanup, a stronger
output protection than the original. Fixed `/tmp/stats` requires exclusive host
admission; the bind-to-acknowledgement abrupt-death window is not claimed leak-free.
Shared logging helpers remain pending. Evidence:
`2026-10-19-stats-runtime-tools/FINAL-HANDOFF.json`, `frozen-och3hnSA`.
Inventory is 209 replaced-local, 3 partial and 3,368 pending baseline sources.
Fresh post-deletion builds and validation are now CI-owned under the user's
2026-10-19 instruction; no further local regression loop is required or claimed.

### First grouped cloud validation and next controller batch (2026-10-19)

Published `2176c4a518ea087d3f69cd10b598a872592bc2f2` in PR #343. Fast checks,
integration and mapped user docs passed. The [Rust run](https://github.com/bin9208/openpilot-rust/actions/runs/39188181505)
is not an accepted aggregate. Three failures were inspected from completed job
logs while other jobs continued:

- Workspace formatting required the statistics module declarations in rustfmt
  order. The earlier local command sequence did not propagate this failed check;
  subsequent formatting checks use an explicit successful exit before diff checks.
- Telemetry's collision scenario differed from the original fixed capture. Its
  preexisting file used implicit host umask while the capture records mode 0664.
  The fixture now sets that input mode explicitly; all generated output modes
  remain exact. The old failed run did not retain the temporary mismatching
  payload, so the next run also prints expected and observed values on failure.
  This repair is pending CI, not a demonstrated runtime defect.
- Navigation's 26-case and dynamic-reference comparisons passed. The combined
  retirement test then failed because the job omitted `REGISTRATION_BINARY`.
  That job now builds both actual registration artifacts and exports both paths;
  no test or comparison was removed.

The next grouped batch introduces native controllers for `check_bridge.py`,
`check_soundd_daemon.py`, `check_logmessaged_startup.py`,
`check_bluetooth_files.py` and `check_bluetooth_journal.py`. All five originals remain required independent cloud
controls. The bridge preserves both 160-packet directions, subscription changes,
private network namespaces and original deadlines. Soundd retains all seven
owned-PortAudio scenarios. Logging retains the legacy first-error loss and both
ready observations; its Rust paired audit checks raw packets, disk records and
the interpreter-free native execution trace. Bluetooth files compares all 15
values against files produced by the unchanged original controller. Bluetooth
journal checks all 721 operations, exact values and inode rewrite decisions,
normalizing only session IDs as the original does.

Only source review, formatting and syntax checks were performed locally for this
new batch. Bridge review corrected an invalid signal constant, nested helper
group ownership, blocking FIFO admission, non-UTF8 metadata and cancellation
during receipt publication before compilation. Added CI requires their actual
builds, units, process tests, original pairs and strict Clippy. No new Python
comparison implementation was added. These five source-ready units do not
increase the retirement count: it remains 209 replaced-local, 3 partial and
3,368 pending. No device test or measured CPU saving is claimed.

Soundd review also restored stdin inheritance through both worker levels,
rechecks source/artifact identity after potentially blocking stdout, and retains
the receipt descriptor through ownership-checked cleanup. Its source-ready test
set now contains 19 checks, including blocked-output mutation and stdin forwarding;
these are awaiting cloud execution, not reported as passed locally.

At `1364e48c`, Fast checks reached the timed CI preservation tests and failed
because the unchanged logmessaged startup command now writes to the paired proof
directory and names its collector through an explicit variable. The immutable
captured baseline remains unchanged. The guard now admits only this exact
command relocation, requires the production collector and proof definitions,
and includes removal mutations for all three. Cloud verification of this repair
is pending; no logging runtime failure is inferred from the routing test.

The same run reached three new controller gates. Bridge passed seven integration
and six unit checks plus Clippy and its actual native controller, but the paired
reader and artifact uploader could not read root-owned 0600 reports. CI now
returns only that private output directory to the runner after namespace work,
including failure cleanup; report permissions and assertions stay unchanged.
Logging startup passed eight integration and one wire check, then strict Clippy
rejected an unnecessary unit expression, which was removed. Soundd stopped at
an incorrectly spelled generated Cap'n Proto module import; it now uses the
existing `h_u_d_control` spelling. Later checks did not run, and all three fixes
require a new cloud result.

The following grouped batch adds calibration Float32 and Panda safety-padding
controllers without deleting their original Python controls. Calibration keeps
all 25 byte inputs and dynamically compiles the retained C++ getFloat method;
its cloud pair compares full reports, generated source and CLI behavior. Review
corrected cancellation after handler removal, unset-versus-empty PATH lookup and
Apple-only FIFO test availability. Panda retains all four full/truncated and
padding controls, including the deliberate source/native truncation distinction.
Its cloud pair uses the existing compiled probes and full transcripts.

These new controllers have 16 calibration and 13 Panda checks prepared, not
locally executed. Their shared registration and CI integration received static
review. Modem #398 adds 14 private-process regressions with a grouped CI baseline
at 1364e48c: the old EOF acceptance assertion must actually fail before fixed
checks run. The coordinated target cache is reused, then the current launcher
is rebuilt. No other baseline failure count is presumed. Native helper cleanup
and cancellation changes, the exact 2s deadline and inherited behavior boundaries
are recorded in `rust-modem-helper-lifetime-398.md`. Inventory remains 209/3/3368.

The 1364e48c Bluetooth file gate passed all six tests, Clippy and the actual
15-case CLI including all Unicode scalar values. Journal passed eight tests
(including its original 721-operation pair) but strict Clippy then rejected an
unused PermissionsExt import left after immutable launcher conversion. Removed
only that import; the final separate CLI and subsequent original connectivity
comparisons were not reached. Source retirement still waits for complete evidence.

On the later `edb` bridge batch, Bluetooth files passed its seven sealed tests,
strict Clippy and the same-ELF present/absent comparison in
[Actions run 39477200877, job 118816129917](https://github.com/bin9208/openpilot-rust/actions/runs/39477200877/job/118816129917).
The later UI CMake failure was unrelated. Artifact 12291885056 has ZIP SHA-256
`b681cd8a57158eebf10c7445cc8f5b581ec22bc17efe15390322fc42d1543c22`;
the two 34-entry lanes each total 21,734,088 identical bytes. Checker ELF
SHA-256 is `4a454ebd7521e5bbf94661f97d0c30b1131fce074b85aed9817398a50e005c75`
and fixture ELF SHA-256 is
`df7a42b9e709682ad569b3754f0838a0477dc80646c5cfafbccc6cf9318acff8`.

That evidence authorizes deletion of only `rust/tools/check_bluetooth_files.py`.
The post-deletion source now requires a fresh CI real-CLI trace under a disabled
PATH, with exactly the sealed checker and fixture exec attempts. The trace and
audit are written to `BLUETOOTH_FILES_EVIDENCE`; the immutable original Python
observation and its provenance remain separate. Post-deletion CI has not yet
run, so this is a source-ready retirement rather than a cloud-validated one.


### BlueZ prompt policy controller and cold node280 coverage (cloud pending)

The next source-ready controller covers `check_bluetooth_bluez_policy.py` without
removing its unchanged original. Seven prompt kinds use all 33 fixed values,
808 independently parsed Unicode 15 Digit/Decimal points and 68 six-digit
sequences: 6,363 ordered cases. Native policy expectations use the pinned
Unicode property source, never production Bluetooth policy or digit tables.
The required UI gate builds the native controller, runs its unit/regression
checks and strict Clippy, then compares complete JSONL, source answers, stderr,
receipts and the full digit table with actual Python 3.12 on the same hashed
candidate ELF. Actual original output is retained visibly for later sealing.
Source identity and binary identity remain held through final receipt
publication, with original 30-second candidate deadlines. No functional result
or source retirement is claimed before the required Actions run.

The compiler corpus adds a cold prefix280 test from independently captured
LLVM18/20 results: source is reconstructed from the pinned full ONNX model,
compiled natively and run from its six external input recipes. Captured
intermediate buffers and original objects authenticate provenance; they are not
native compilation inputs. The zero desire case is retained. A second original-only capture with raw f32
one-hot desire rows passed under both providers in
[run 39485648481](https://github.com/bin9208/openpilot-rust/actions/runs/39485648481),
commit `ca42e06a490f3bfd1287f0eb0104048726a15aa9`. Artifact 12293287379
has ZIP SHA-256 `efe890b579566c7fe193c8e5fd49517dada3c9ec197275143b972a9e737786f7`.
Parent independently compared its raw original receipts, IR, objects and changed
terminal input with the compact fixtures. Both providers produced changed output
SHA-256 `47989d3b10646cd7933c72366373ebb3f753222bf2fea9dba653c6ba363d8ec1`.
The required native cases compile and run both inputs under both memory policies;
their outcomes remain pending cloud execution. Prefix93 remains a separate
unresolved numerical boundary.

The narrow Lagd allocation change is tracked in
[rust-lagd-masked-allocation-429.md](rust-lagd-masked-allocation-429.md).
Existing original numerical/daemon checks remain mandatory, and a cloud-only
old/current probe must verify the numerical fingerprint and actual allocation
delta before any measured saving is reported. Local work for this batch remains
source review, formatting, syntax and diff inspection only. Inventory remains
210 replaced-local, 3 partially-replaced and 3,367 pending; device boot,
recording, upload and driving acceptance remain user-owned.


The next Bluetooth enumeration controller preserves all 40 sysfs nodes, the 29
CPython 3.12 whitespace points, 31 accepted paths in actual directory order,
and the missing-directory response. The required UI live pair uses the same
fresh input fixture ELF for the unchanged Python and native controllers. Its
reference compares complete node bytes and ordered source/native maps, removing
only each controller's distinct output-root prefix. Original stdout/stderr and
reports remain visible for later independent sealing. Eight native module
checks cover the complete report, runner/spawn failures, node/addition/absent
mutations, cancellation, foreign output and late binary drift. Their functional
and strict Clippy outcomes remain pending Actions; the Python checker remains.


Panda allocator-padding retirement preparation retains twelve independently
executed raw files from artifact 12289868344, ZIP SHA-256
`40026fdfe4f4a6dcd804f1e48208a1e41c38ddd9cff025fd83c6cb19da996d01`.
The fresh original, native-present and native-absent lanes remain required;
the absent lane additionally audits the exact four native exec attempts under
an unavailable PATH and seals all three ELFs. The complete 21-input original
source-build map is retained separately and must match current declarations and
source bytes. The Python checker remains present pending these cloud results.
A finalization binding gap discovered during review is tracked in
[#430](rust-panda-padding-finalization-430.md), with an explicit old-assertion RED
and fixed four-case gate prepared. This does not increase the retirement count.


The native Bluetooth gesture input generator preserves the 37 ordered cases,
647 deterministic prefix operations, 10,000 seeded random iterations per shared
sequence and all 128 recorded input events. Review caught two exact-input
hazards before execution: integer middle stamps at 0/10 must remain integers,
and recorded decimal timestamps need correctly rounded lexical f64 parsing
rather than serde_json's default numeric conversion. Both are corrected in the
source; the tests inspect raw numeric lexemes. A mandatory UI cloud gate runs
the unchanged original gesture checker against the same fresh candidate ELF,
compares the entire generated input JSONL byte for byte and then compares the
candidate's complete output and stderr for that native input. Source/ELF hashes
must remain unchanged. Its outcome remains pending Actions. Both Python helper
and checker remain present, with no inventory increase or functional completion
claim. Local validation for this batch is limited to formatting, syntax and diff.


### Prefix93 original fused-kernel localization (2026-10-21)

The original-only ownership capture at `0c2d4d0806b6c802f81dcfb52066c20451ea6ba8`
passed [run 39488465388](https://github.com/bin9208/openpilot-rust/actions/runs/39488465388),
artifact 12294054127, receipt SHA-256
`e3152225d774365df1eedecc4565a5d73a0e061b589149847d70ffe27766f3cc`.
The prior run 39487728226 failed the unchanged IR seal because the capture
omitted historical CPU_COUNT=16; restoring that configuration passed without
changing the expected IR or output. This is the driver-monitoring prefix93,
not the driving model's independent prefix280 test.

A separate original-kernel diagnostic at
`1f28d6a765beb27d3b9a886d650bf8d09643c7c4` passed
[run 39490770391](https://github.com/bin9208/openpilot-rust/actions/runs/39490770391),
artifact 12296155986. The preceding run 39490310168 failed before execution
because PYTHONPATH omitted the repository root; only that path was repaired.
Its receipt SHA-256 is
`b8869972f49d94a3d33e2084ada2c9400cf4c16e67e9d0e13fdc458774ce42f7`.
The original sealed object and the copy instrumented to expose Gelu scalars
both retain exact historical final-output SHA-256
`7307a9b9129f7650c5793a6098dfb30f1830699e24f7a94c3936d6d6c9b57ee5`.
All source/provider/input and instrumented IR/object hashes are recorded.

Parent independently rehashed all five diagnostic payloads and compared the
92,160 raw f32 words against the independently materialized Gelu92 input:
92,114 match exactly and 46 differ. The established channel-major/group/lane
layout has far more exact matches than the two tested alternate layouts. One
mismatch occurs in each of the three channels with final output differences:
channel/spatial 46/54, 568/21 and 704/78. Therefore the discrepancy is already
present before ReduceMean in this output-preserving diagnostic. This does not
yet identify a Conv or Gelu arithmetic operation. The native fusion fence and
all exact numerical criteria remain unchanged. The temporary original-only
capture workflow stays on its isolated capture branch and is not part of this
PR's implementation or a Python retirement claim. No original or native kernel
was executed locally for this follow-up.


The Bluetooth permission-command test helper now has a native source port and
required cloud process pair. It preserves arbitrary argv logging, Python Unix
surrogate escaping, unknown modes, fail-<second argument>, ten-second sleep,
self-SIGTERM and complete 1 MiB output on both streams. Short argv is logged
before its failure and after any mode action; Rust-owned argument diagnostics
are not represented as Python traceback parity. Each child has a 20-second
owned-group deadline, and all ten actual original/native pair cases retain raw
logs, streams, status and timing in the UI artifact. The interpreter, original
script and native ELF are sealed before/after the pair. Source review caught
and fixed initially restricted argument admission and the loud/error stderr
assertion. The original helper and both Python callers remain until actual
cloud results and caller migration. No retirement count changes.

[Issue #431](https://github.com/bin9208/openpilot-rust/issues/431) tracks the
messaging intermediate-vector allocation candidate. The new borrowed input
route keeps complete batch parsing before state mutation and existing update
callers. A source regression covers two-topic processing and malformed-second
message rejection. The required selfdrived gate builds exact old/current
messaging libraries and compiles the same standalone Rust allocation probe
against each checkout's own unambiguous cereal/capnp dependencies. The package's
unsafe-code prohibition remains intact. Four receipt rows cover warm empty,
single-topic, two-topic and malformed batches; their complete fingerprints must
match, the empty control must stay equal, and both nonempty paths must use fewer
Rust allocator calls and requested bytes. This is an unmeasured hypothesis;
Rust may reuse the previous collection's allocation. Failure of that premise
requires reconsidering the candidate, not lowering the gate. Existing numerical,
wire, messaging/selfdrived and original comparisons remain required. Bounded
source review of timed and retained logging producer paths found no additional
new defect; it does not clear all modules. No local functional execution occurred.


The next original-only Conv/Gelu diagnostic at
`10beba0628ea9862f898d95554e1312c6da103be` passed
[run 39491994973](https://github.com/bin9208/openpilot-rust/actions/runs/39491994973),
artifact 12296012647, receipt SHA-256
`d0a663177306dcca8d1ec281fb4164e27bfb63c12c3aec88e36bb108df14f369`.
All 92,160 pre-Gelu Conv values match a separately recaptured original Conv91
buffer exactly, SHA-256
`f3fbba7c06e6304d7e14a98da1f34a9aeb269cfc3e54459a2e6abc31d7dd9aed`.
The Gelu differences and original final-output hash stay unchanged. Parent
rehashed seven diagnostic payloads and independently checked complete Conv
buffer and final-output byte equality. The first demonstrated mismatch is
therefore within fused Gelu arithmetic for this input, not Conv or ReduceMean;
the exact Gelu suboperation is still under investigation. The fence remains.


## 2026-10-21 controller retirement integration checkpoint

BlueZ policy and enumeration original/present/absent proof passed at source
`c7005ce1dfb7aeaaec74aa7d41d780df87651c7b`, [run39508434006, job118923150264](https://github.com/bin9208/openpilot-rust/actions/runs/39508434006/job/118923150264).
Parent inspected artifact12304089348 (ZIP SHA256
`c6dcff3ac13beb53e1e2eb8b0ba14577bb70688dfa0a918e9ce9fd84d56ef716`).
All three BlueZ lanes retain identical 6,363 cases, raw candidate responses and
independent source observations; 808 digit code points cover Unicode15.0.0.
Absent execution is exactly the checker plus two native fixture invocations.
Enumeration retains40sysfs nodes/31accepted devices with no Python exec attempt.
The two historical checker files are now removed; combined runtime callers use
native executables. Reappearance is rejected. Fresh post-deletion exact-source
checks are mandatory and still pending at this checkpoint. Inventory:
213replaced-local /3partial /3,364pending; this is not whole-project completion.

The full audio job at source`f60e4ac0a655592aaef659f35cc354b86ffa09a0`
[passed](https://github.com/bin9208/openpilot-rust/actions/runs/39505980261/job/118914882870),
including the old finalization behavioral failure, current5worker/9controller/
8process checks, strictClippy, seven original/native scenarios and PortAudioMiri.
Parent inspected artifact12304571676 (ZIP SHA256
`23e074eef387fbff6bf1262993e0854536833183f5c4a593611bb6f78f8a8652`).
The original-only seven-scenario report is retained with provenance; a new
same-ELF present/absent 22-exec audit is pending. The Python soundd checker is
still present until this stronger proof passes.

The new full native paramsd controller preserves1,202continuous/eight restart
publications, atomic cursor gating, persistence, GPS and bounded shutdown.
An independent Rust reference decodes every actual packet against original
source rows. Its functional cloud comparison is pending; both original Python
controller and input generator remain. No runtime/device acceptance is claimed.

Diagnostic attempts also identified missing focused-job prerequisites (pytest
configuration, capnp and zstandard), rather than demonstrated production defects.
These setups must reuse the successful main-job commands. The isolated fused
Mean candidate compiles but still rejects its real cold prefix before execution;
it remains outside this branch's committed implementation.


### Full paramsd controller cloud acceptance and new soundd precision defect

At exact source`ba5def0ab46de9cab2d3542af573625761068512`,
[job118930155954](https://github.com/bin9208/openpilot-rust/actions/runs/39510538880/job/118930155954)
passed the scoped real-daemon FD gate and full original/native controller proof.
All1,202continuous and8restart raw packets decoded against independent source
rows; saved index1199, per-lane restart bytes, GPS, gate counts and normal
INT/TERM shutdowns passed, as did strictClippy. Parent reviewed artifact12305212845,
ZIP SHA256`a9a6fc6dcbf15af20696d832169ba81cf5cebe4364546466ccaa1b54b2a15ce2`.
This preserves the original controller and generator until later absence and
caller-conversion gates. No physical runtime acceptance is implied.

The same source's BlueZ post-deletion reference passed both tests and all6,363
cases with exact native exec audit. Enumeration's new reference failed compilation
because include_bytes! already returns a reference; the extra `&old_raw` was
removed. This compilation failure is not numerical/functional failure or a pass.

The new soundd absence proof executed all original/native-present/native-absent
modes, then its independent reference found a last-bit median discrepancy in
the original inactive lane. [#434](https://github.com/bin9208/openpilot-rust/issues/434)
records the raw three timestamps and summary. The candidate enables
`serde_json/float_roundtrip` within the soundd feature and keeps exact comparisons,
with raw timestamp plus summary-number regression. Intended oldRED/currentGREEN
and complete proof are pending; no numerical tolerance was added.

Bluetooth journal preparation retains721ordered original normalized row hashes
from actual artifact12304713010 and requires fresh original plus native
present/absent lanes, exact two-executable argv audits and ELF hashes. Its Python
checker stays present until that actual proof passes. These pending preparations
are not additional retired inventory entries.


### Soundd absence accepted; historical caller retired

At exact source`f19ef5ad4cb56b9a7bace81fdeb34b6325e704eb`,
[job118935976457](https://github.com/bin9208/openpilot-rust/actions/runs/39512282992/job/118935976457)
passed the intended old-parser summary-bit RED, repaired exact regression,
complete original/native-present/native-absent seven-mode reference and strict
Clippy. Parent reviewed artifact12306223105, ZIP SHA256
`249dcaa0ff9c5c82a27c8cbc93bbddf83ecc79f8320546b8e4c4a373576293b1`.
Both native lanes executed exactly22successful native programs with identical
four ELF/library hashes; source maps differ only by the historical checker
(21present/20absent), and all raw event checks/expected exits/open counts passed.
`rust/tools/check_soundd_daemon.py` is now removed. The following mandatory
post-deletion gate uses its sealed independent original observation, all seven
fresh native modes from the checkout and20-source copied root, both execution
audits and exact parser regression. Reappearance is rejected. This newer gate
is pending exact-source cloud validation; physical audio remains untested.
Inventory is214replaced-local /3partial /3,363pending.

The latest full PR source a993 also exposed two validation maintenance failures:
workspace module-declaration formatting and a strictClippy `get().is_none()`
in lagd_ci. These are corrected without changing the validated estimator.
Panda passed its earlier source comparisons, then its known baseline finalization
RED was followed by a missing hosted`rg` executable. Those three literal checks
now use`grep -Fq`, retaining exact assertion/exit requirements. This failure
was not a new #432 timestamp reproduction or a fixed #432 root cause.

### paramsd atomic test source retirement (2026-10-21)

Sourceba5def0a [full cloud job118930155954](https://github.com/bin9208/openpilot-rust/actions/runs/39510538880/job/118930155954)
passed all5unchanged Python cursor cases,7native foundation tests, actual
daemon FD handoff, full continuous/restart comparison and strictClippy. The
Python test is removed and its normal/wrapped/overflow/rejection assertions
remain in native tests, including batch counts on both wrap paths. CI removes
the old pytest invocation, retains native execution and checks source absence.
The live Python StartupGate helper and other comparators remain pending.
Inventory215replaced-local /3partial /3362pending. Post-deletion fresh-SHA
functional evidence is still required.

The fresh62e8c235 fast job118943013713 exposed a disk-policy classifier
collision: the config absence step's512MiB guard was attributed to the
config controller because its following command used the same Cargo feature.
Classification now uses the explicit step identity for the controller,
retaining both absence guards[128,512], all controller guards and total458.
The failing cloud output is the regression evidence; repaired CI is pending.

### paramsd reusable fixture generator preparation (2026-10-21)

A native generator now prepares all7reusable scenario sets plus continuous1202
and restart8frame input sequences, including all Cap'nProto wire bytes and
rawf64state bits. Independent original construction and actual original-controller
inputs are compared in a required estimation gate. The real generator runs under
emptyPATH/strace with one exactexec; input/ELF seals and CLI failure tests remain
required. Cloud compilation/byte equality is pending. Original Python fixture,
arbitrary-service event API and live callers remain; no additional retirement
is counted. A272-byte synthetic persisted packet from artifact12305212845 is
retained only as provenance-bearing input for focused generator comparisons.

### Source-owned prefix93 closure accepted for opt-in compiler capability

Exactsource13b3ff14 passed the independent cold final-output gate and138tests
across19affected target suites in run39515566390/job118946776183, plusstrictClippy.
Parent verified actual3,840-byte output equality and artifact12307294357 archive
69440cc8…; details are in rust-prefix93-fma-diagnostic-296.md. The default
fusion fence remains. This capability does not retire remaining model-export,
fulltinygrad or vendor Python, and does not establish device/CPU results.

### Native paramsd loop input preparation

The native input family now also covers all39loop cases:22legacy cache
migration byte encodings,12V2 variants,3malformed/suppression inputs and two
ordered1210-frame GPS lanes. Fresh original helper output must match the
actual original checker's input and every native packet byte, including event
validity, list allocation and message reversal. Original Python and callers
remain. No source is counted retired for this preparation.

Source-policy d219 job118950166180 passed native fixture byte/CLI tests and
strictClippy, then exposed a policy test looking for the step's display name
in shell commands. The name is now checked in the structured step field;
all actual command, capture, seal and mandatory-gate requirements remain.
Repaired exact-source policy and new loop byte validation are pending.

### soundd post-deletion actual native gate accepted

Exact sourced21944e6391344d5b807fafe503c8853ea245b1b passed
[run39516609369/job118950165958](https://github.com/bin9208/openpilot-rust/actions/runs/39516609369/job/118950165958).
The parent independently verified artifact12308905066, archiveSHA256
`fdd2f3641c4cce90122d03b016ca04dc8d3a2eaab4ef6e7df8cc491e1c81e5e8`: both
roots ran7modes with22exec attempts each, matching4ELF hashes and20retained
source pins. The retired Python checker is absent in both roots; only its
historical hash remains in provenance. Native finalization/process contracts,
the median-bit regression, full mode reference and strictClippy passed.
Physical audio and vehicle boot/recording/upload remain user-device evidence.

## Bluetooth config, journal and gesture generator retirement (2026-10-21)

The complete pre-deletion Bluetooth proof at source
`d21944e6391344d5b807fafe503c8853ea245b1b` passed
[run39516609369/job118950166100](https://github.com/bin9208/openpilot-rust/actions/runs/39516609369/job/118950166100).
The parent downloaded artifact12310560213 (51,785,801bytes), ZIP SHA256
`f4992ccbd9712d041de1369b23394978cc994c28329b58c21b9e797cfdb6861e`,
and independently checked the exact source, all14 actual suite receipts,
253 config cases,721 journal operations in each native lane, two successful
execs per journal lane and unchanged ELF seals. All37 gesture cases retained
260,012 steps and1,024 tokens. Original/native/absence input bytes match
`b5eefe42e5f5fb135b71ef50e97a7905d5f41a4d943b7b5e3bcebddf1d0d8166`;
both live original Decoder outputs match
`ddbdc78183b07d39e77f61002153c935c32a25becf71601e28f6c94c0c703705`.

Only after that proof, three Python files were removed:
`check_bluetooth_config.py`, `check_bluetooth_journal.py`, and
`bluetooth_gesture_cases.py`. Their native tools now require absence at
admission and during evidence publication. Config compares the independent
sealed253-case input/response corpus; journal retains all721 original row
digests, every rewrite decision and source identities. The gesture generator
requires the sealed original raw input hash; the independent original Decoder
still executes freshly in both roots. The14-case suite now dispatches journal
through its native checker. No Python source was embedded or renamed.

Inventory218replaced-local /3partial /3359pending. Post-deletion exact-source
cloud execution is pending. This does not establish repository-wide retirement,
full model/compiler coverage, dev integration, ARM packaging or device acceptance.

## Complete paramsd input wire and repaired source gates accepted (2026-10-21)

Source `200ba3fd91c31c448df0bd03b53e0f1a2b7e80af` passed
[run39519153139/job118958465021](https://github.com/bin9208/openpilot-rust/actions/runs/39519153139/job/118958465021).
The parent downloaded artifact12310311716 (322,999bytes), ZIP SHA256
`670f2081c86e1ea898e3ea62b072d9812d32534cc94de46b20e8f7df4865b5c0`,
and independently verified all parsed metadata and raw packet bytes for all39
loop cases, seven estimator scenarios,1203 continuous frames and nine restart
frames against freshly generated unchanged-original inputs. Both native CLIs
had exactly one successful native exec and no child exec. The fresh original
inputs were also compared against actual original controller input captures
from run39514415599/job118943760823, artifact12306774260, capture source
`4a3336f564e5766b07627633a1b9d72038693d92`.

All five generator tests and strict Clippy passed. The same job passed all13
disk guards,66 Rust workflow policies, the soundd policy, seven atomic-gate unit
cases (the hardware/IPC-dependent ignored case remains separately exercised
by the full daemon proof), four Panda finalization regressions and strict
Panda reference/finalization Clippy. This establishes the repaired policy and
reference compilation at this source, not a new actual Panda timestamp proof.
It does not retire the Python fixture/source/controller family or substitute
for the new paramsd sealed-numeric full daemon job.


## Bluetooth post-deletion acceptance and LLVM 20 cloud evidence (2026-10-21)

Actual checked source `b43ac10d78c20029c86028e96811678f8cff5a1d` passed
[Bluetooth post-deletion run39521331522/job118965606861](https://github.com/bin9208/openpilot-rust/actions/runs/39521331522/job/118965606861).
The diagnostic workflow commit is separately `cc560cf36a9b5ea302cba5910c2d1f525968b8e7`;
artifact SOURCE_COMMIT and both checkout assertions identify b43ac10d.
Artifact12311752216,51,565,169bytes, archive SHA256
`c50973c0e3ca976acfdacc5cc55515c7f0872f21e9ff23b2186d1de75690e06b`
contains all14 passing caller receipts,253config cases (97accepted),721journal
operations,37gesture cases/260,012steps. Same-ELF native roots with the retired
Python present/absent have identical config/journal reports and37gesture inputs.
Exec traces contain exactly checker+fixture for config/journal and one generator
for each gesture lane, with no failed exec. Journal pre/post ELF hashes agree.
The source-policy companion job118965606642 passed13disk,68isolation,2lagd tests.
This accepts the three removals from61f4f275; inventory remains218/3/3359.

The older full PR
[LLVM20 job118943760505](https://github.com/bin9208/openpilot-rust/actions/runs/39514415599/job/118943760505)
passed108tests across30suites. Event head was62e8c235ccc1fa6e109d3c02d9bd76102047efba;
actual merge checkout4a3336f564e5766b07627633a1b9d72038693d92.
The graph/binding/cold-prefix suites passed92tests, primitive/provider suites16.
Pinned LLVM archive/library hashes and ldd have been inspected. Artifact12311119743
is423bytes, SHA256 `6e1de22f25d3528119ef699f20cbfe3a8644b4dd82c1c500d030adb5b45ad66c`;
it contains the dependency listing; test evidence is the job log. These are
selected compiler-path results, not complete Python retirement or device evidence.

Source13d20653 registers the same-ELF logmessaged startup retirement test. The
main workflow additionally requires live unchanged-original observations, native
present/absent roots, all three packet/disk observations and seven exact native
execs per lane; source/ELF seals precede the original invocation and are rechecked.
The new469 disk-guard and mandatory-step policies await cloud execution. The
historical checker remains until that proof passes. Local checks were Rust
formatting, YAML/shell syntax and diff review only.


## Logmessaged startup checker retirement (2026-10-21)

Actual source `13d2065301fea64b1f0af0549ab69480a128234a` passed the independent
original/paired-native/present/absent proof in
[run39523666271/job118973216977](https://github.com/bin9208/openpilot-rust/actions/runs/39523666271/job/118973216977).
Workflow head was separately `29a08fcf0cca90d0442f0e4037f36f248a57863c`.
Artifact12311923940 is2,553,541bytes, archive SHA256
`d2cd07c56ba9e74e066c45e38338d7905be2e8a3854e4ad2f302513cbb0ecdac`.
The parent verified all18 newly retained data files are byte-exact members of
`paired/original/`, with no copied Python source or native-derived expected data.
Fresh original, native and both helper-presence roots agree across legacy-source,
ready-source and ready-native: log/error counts1/0,1/1,1/1. Packets decode the
same valid Event text, disk rows differ only by validated UUID, and collector
exit code/signal is-2/2,-2/2,0/2. Every native lane has exactly seven successful
execs without Python/compiler execution. Ten checker tests, one worker test,
one paired reference and one sourceabsence test plus strictClippy passed.

`check_logmessaged_startup.py` is removed after that preproof. The required main
step now executes fresh current and copied absent roots, keeps all worker,
CLI/error, readiness, complete packet/disk and exit checks, and compares against
those18 independent original-only files. SHA256SUMS digest
`cee8e19032542bc4851d0ce6bfebd9b71dde9184b19bdc5b0fb23e06e974d0dd`
is compiled into both reference tests. Source/ELF and root identities remain
sealed, exact execution vectors are required, and both roots must lack the old
helper including dangling symlinks. The new post-deletion gate is awaiting
fresh cloud execution. Inventory219replaced-local/3partial/3358pending.
Local validation is formatting, shell/YAML syntax, fixture data/hash review and
diff inspection only. No device, full conversion or integration is claimed.


The logmessaged startup removal passed its actual post-deletion cloud gate at
source `4492987ca41d1848a80e9772443964b031bc2ddb` in
[run39525293160/job118978372228](https://github.com/bin9208/openpilot-rust/actions/runs/39525293160/job/118978372228).
Workflow commit was `66ac24095796aba36e16979177e794b5d35bcebf`; exact checkout
assertion identifies the source independently. Artifact12313616215 is2,445,745
bytes, archive SHA256 `72611c218fa89542b179ef47ca56aea98869f3535dcbdca0f6711bfdfab563d2`.
Ten checker tests, worker library, independent original reference, current/absent
proof and strictClippy passed. Parent inspected all three observed/collector-exit
records, original disk equality after UUID validation,96-byte packet counts,
seven successful native execs per lane, identical reports and unchanged root
metadata. The cloud tests decode all raw packets and compare the exact expected
Event contents. Both source roots genuinely lack the retired checker.
The companion source-policy job118978371946 passed13disk/69isolation/2lagd.
The main artifact now also writes its SOURCE_COMMIT directly; the accepted
4492987c artifact's source was established from the checkout assertion/job log.

## Panda padding checker removal after source20a proof (2026-10-21)

Source20a8f52e cloud run39529140105/job118990740395 succeeded with fresh original, sameELF native checkout/private-absent roots, full sanitized probes untraced and plain traced builds bound to identical21source hashes. Artifact12314188013 SHA256 `3153e98dd6ff053928cff93c4549bc105217632d7a5aa103a440147cfe1102d0`. Parent independently audited all five reports, complete transcripts,9requests per lane and exact4exec vectors per plain lane. See rust-panda-sanitizer-trace-440.md.

The actual Python checker is now removed. Maintained reference/retirement tests verify the SHA-sealed12original-only raw data files; no Python is copied, downloaded or executed for the removed controller. The original negative artifact phase matrix is separately sealed: all18entries independently match edb42bf0 artifact SHA256 `40026fdfe4f4a6dcd804f1e48208a1e41c38ddd9cff025fd83c6cb19da996d01`. Native CLI exits, all failure phases, cancellation, finalization and mandatory interpreter-free absence checks remain. The historical live-original proof is recorded separately from postdelete captured observations, without fabricating a fresh original exit status.

Inventory220replaced-local/3partial/3357pending. Actual deleted-checkout cloud proof, latest whole PR and device acceptance remain pending.


## Calibration float checker retirement after fresh original proof (2026-10-21)

Accepted pre-deletion [run39533224232/job119003782752](https://github.com/bin9208/openpilot-rust/actions/runs/39533224232/job/119003782752), source7b5ebbb7243a0f74cb3b075067a5dd50506b4927, workflow80113fb7c98081e1cbfae401a60350a6aa9525b9. Artifact12317055106 ZIP SHA256 c169d7ef9864c0c3ccc3d1a23e91cf56d02962d3153b5cf185734a79052871d2. Fresh original25-row report, generated C++ and stdout,12 raw CLI phases, same native checker/candidate ELFs across real source-present/private source-absent roots and56 successful execs per lane passed; helper admission includes actual GCC libexec paths. Parent independently compared retained report, C++, original stdout and all100 supplementary native stream files to the downloaded archive. Supplementary streams are explicitly native-lane data, not original-only outputs.

Removed rust/tools/check_calibration_float.py and its obsolete source-present retirement target. Native tests keep the independent original report/CPP/CLI receipts, all20 CLI admission cases and failure injection. New postdelete target requires actual repository/private-root source absence, both current ELFs sealed before/after, cold C++ execution, full25-row comparison,100 streams and56-exec audit in each root. Current rebuilt ELF hashes are measured; historical ELF hashes are provenance only. Main workflow publishes postdelete evidence in a sibling directory so the later exclusive CLI output remains fresh. Local Rust syntax/format/diff and archive-data inspection only. Exact deleted-checkout cloud proof, latest full PR, whole project conversion and device acceptance remain pending. Inventory221replaced-local/3partial/3356pending.


## Hosted full ARM build deadline (#445, 2026-10-21)

[Full PR run39524132398/job118974708126](https://github.com/bin9208/openpilot-rust/actions/runs/39524132398/job/118974708126) checked merge3835653ecdea12c08f0cb9cb2807b0a493f20bb7 for PR head8e8b1e8f. The job started00:58:52Z, logged cancellation02:59:22Z and completed02:59:27Z, matching its120minute deadline. It had reached the final generic/ION UI build after compiler/static-C3X/encoder progress. This is not an ARM success; no compiler failure appeared at the cancellation boundary. [Issue445](https://github.com/bin9208/openpilot-rust/issues/445) tracks raising only that hosted deadline to180minutes with all stages, disk guards and artifacts preserved. Fresh full ARM completion remains required; no local/self-hosted workaround or device test was performed.


## Paramsd loop two-tool actual source retirement preparation (2026-10-21)

After accepted predelete run39535390950 at5f6104be, removed onlycheck_paramsd_loop.py andparamsd_loop_fixture.py. Retained independent original C++ source JSON(7,763,306bytes raw), originalPython request(4,430,072bytes raw), stdout/stderr/status/process log/results from artifact12317675555 ZIP SHA2561111f01652e8d9899cd0c1669bc589498487bb6b3b74a064e8c429abc64ffcf4. Parent compared all retained bytes directly to the downloaded archive; current native output is excluded from the expected fixtures. Actual checkout/private-root absence, same currentELFs, exact2execs each,39cases/2420frames/4saves/216GPS writes and finalization regressions are mandatory. Combined fixture workflow decompresses the original request with nativebzip2 and pins both compressed/raw hashes; other still-live Params/daemon comparisons remain. Local syntax/format/diff/data inspection only; actual deletion CI pending. Inventory223replaced-local/3partial/3354pending. Wholeproject/latestfullPR/device acceptance remains incomplete.


## Updater actual three-source retirement prepared (2026-10-21)

Source505cc30c520ddb4a0bbbd3fbe82e7cd8c2584eb3 was checked by
[run39543788680/job119037266203](https://github.com/bin9208/openpilot-rust/actions/runs/39543788680/job/119037266203),
workflow3ef960060b7bbb98266b84241e287ef566b324f4, artifact12320896721,
ZIP SHA256 ebfe3e6294e23cc5467c4c89f1ebefd45d9d63f46d3106a8f90f9d95930b50f5.
Parent independently compared all five fresh original source JSON to sealed
observations and all15 native JSON across direct/checkout/private lanes to the
fresh old CLI native lane. Original16 and native15 tests passed; native tests
retain every16 original assertion case. Each root had1,089 classified successful
execs without Python and before/after seals matched; strict Clippy passed.

Removed only rust/tools/check_updated.py, updated_source.py and
rust/tools/tests/test_updated_comparison.py. Maintained caller, native feature,
all original regressions and default CLI move together; unchanged updater process,
daemon and AGNOS gates remain mandatory. updated_fixtures.py remains a live daemon
dependency and a passive native input pin. Actual deleted-checkout execution,
whole startup-services/latest full PR/ARM and device acceptance remain pending.
Inventory229replaced-local/3partial/3348pending does not imply whole conversion.


## Desire actual controller retirement prepared (2026-10-21)

The unchanged Python CLI and native controller compare all24585 original rows,
47scenarios, full state/discrete/array fields and original1e-10 numeric tolerance.
At source049a3f9c run39546177049 the main fresh pair passed: request143229930bytes
SHA6ae79545af6ca232d603520cf05d0a2f83ed207a14e3bbc4e0518154f59cb570;
reference96524611bytes SHAea0b820405f12e2922db8efe41f780b9064c6e76d347529b44b6ec621ac41847;
actual89830766bytes SHA71e098692d451676bf1763121a4337ff50c9a4e82c3476c4db0a1422016c2782.
Parent streamed both lanes from artifact12321509065 and confirmed all hashes.
That whole job failed only at its failure-injection fixture, repaired bya20ad0fa.

Before removal, source1de0b0d7fbe86e6855d58c8fa9dbb038b5d44126 passed
[39548303946/job119051425025](https://github.com/bin9208/openpilot-rust/actions/runs/39548303946/job/119051425025),
workflow32b4379b709b2a5deb0267ff77d4df28df3f7a15.
Artifact12323365248 ZIP SHA256
e0abcb079d7cf91e0fc52cd7e1318bff106662ef3fd5ff6ff91b33dc1bbe06cc.
83library/4integration/1fullsame-ELF tests and strictClippy passed. Both roots had
exactly2successful execs and byte-identical report/request/reference/actual/raw
streams, with unchanged input seals. Independent CLI capture20 cases includes
validraw0xff paths. Status/channel parity holds; help/error wording is deliberately
not byte parity. Parent sealed only original observations, SHA
ac3e8ec4a5117054c073c8761d454ef60a4b599ab16beb9efd647b78364711cf.

Removed only the Python checker; replaced its maintained workspace caller,
kept all27 still-required original dependency pins and Bluetooth command/JSON
gates. Actual deleted-checkout proof now explicitly requires checkout source
absence and replays all20 independently captured admissions. That postdelete
cloud proof remains pending; no model behavior/device acceptance is claimed.
Inventory230replaced-local/3partial/3347pending.
