# Maintained Python tool retirement inventory (#296)

Audit date: 2026-10-14. Source snapshot:
`73a9e01709245de97ffb18b1a079b019bc3f9fd9`.
Tracking: [#296](https://github.com/bin9208/openpilot-rust/issues/296).
Execution plan: [runtime retirement](rust-runtime-retirement-296.md).

## Native scalar filter reference gate

`check-filter-reference` replaces `rust/tools/check_reference.py`. Before removal,
the actual original Python filter classes evaluated every original trace row;
all 24,000 outputs matched with maximum absolute error 3.9968028886505635e-15.
Their numerical outputs are retained as a 192,000-byte float64 fixture, together
with original source/trace hashes and an exact input-corpus digest. No output
was generated from the Rust filter formulas. The native checker verifies the
reference and input identities, runs the native trace with a 30-second deadline
and bounded capture, and retains the original 1e-12 absolute/relative tolerance.

Four native tests pass for positive tolerance, changed/nonfinite output,
changed/missing/extra input and malformed reference data. The full 24,000-output
native command passes with PATH containing no interpreters or executables, and
focused strict Clippy passes. CI and the Rust README now invoke the native gate.
Unlike the former tool, this gate deliberately compares a pinned original-source
corpus; changing original filter behavior requires independent recapture and
provenance review. It does not silently regenerate expected outputs from the
implementation under test. This retires one comparison tool, not the remaining
Python numerical/model toolchain.

The user explicitly included development tools in the Rust conversion. Moving
an existing tool into an archive, changing its extension, fetching it from an
older commit, or wrapping its Python interpreter does not convert that tool.
The maintained tool must gain equivalent native behavior before its Python
implementation is deleted. An external original implementation retained solely
as a comparison oracle remains an explicitly identified validation dependency.

## Count and audit limits

The snapshot contains 3,566 tracked `.py` files: 2,403 project-owned files and
1,163 files with external/vendor provenance. The project-owned files include
1,011 in `openpilot/`, 1,261 in `rust/tools/`, and 131 other developer/tooling
files. External provenance is a classification, not an exemption from tracking
Python sources, build dependencies or package contents. Another 30
SConstruct/SConscript files and eight Cython source/declaration files are outside
the `.py` count.

The following is an initial routing inventory for all 1,261 `rust/tools/*.py`
files, including nested directories. Categories are mutually exclusive and
applied in the stated order. Counts describe names and locations, not a completed
semantic audit, port, test-coverage claim or observation of device execution.

| Route | Files | Selection rule |
| --- | ---: | --- |
| Tool self-tests | 41 | Files below `rust/tools/tests/` |
| Model compiler/export pipeline | 24 | Files below `rust/tools/model_export/` |
| Build, generation, staging and release | 71 | Basename begins `build_`, `stage_`, `package_`, `seal_`, `import_`, `generate_`, or `validate_runtime_` |
| Regression/comparison entry points | 500 | Basename begins `check_` or `test_`, or ends `_compare` |
| Original-source oracle adapters | 209 | Basename ends `_source`, `_reference`, `_oracle`, or `_source_values`, or begins `original_` |
| Fixtures, scenarios and protocol peers | 136 | Basename ends `_fixture`, `_fixtures`, `_cases`, `_scenarios`, `_peer`, `_peers`, `_frames`, `_input`, `_inputs`, `_capture`, `_ports`, `_fault`, `_probe`, `_samples`, or `_vectors` |
| Suite support and specialized helpers | 280 | Remaining files; further per-file classification required |
| **Total** | **1,261** | No file excluded from the route table |

There are 1,082 files directly in `rust/tools/`; the other 179 include tooling
tests and packages such as `card_qa`, `ui_application_qa`, `model_export`,
`carrot_navi_qa`, `xiaoge_qa`, and `wifi_fixture`. Names such as nested `source.py`
or `scenarios.py` fall into the last routing category under these literal rules.
That category also includes real runnable suites, for example
`carrot_server_dashcam_upload.py`; it must not be treated as dead helper code.

This audit counted tracked files and parsed Python imports without importing or
executing the tools. It inspected selected shared helpers and direct Rust/build
consumers. It did not trace all dynamic imports, subprocesses, runtime branches,
or each scenario's assertions. A per-file migration ledger with the native
replacement, CI entry point and preserved cases is still required.

## Shared utilities to migrate first

Unique caller-file counts from static imports identify these existing common
dependencies. Counts consider root-level local modules/packages and avoid
mistaking similarly named nested modules for standard-library imports.

| Existing module/package | Caller files | Useful native boundary |
| --- | ---: | --- |
| `can_source` | 79 | CAN case/receipt format and independent expected results |
| `carrot_server_dashcam_upload` | 79 | Owned HTTP workers, requests, process trees and output comparison |
| `original_params_binding` | 56 | Isolated Params store and original-oracle boundary |
| `carrot_server_dashcam_sync_probe` | 34 | Synchronization probes and observable ownership phases |
| `model_export` | 22 | Model graph/artifact compilation and provenance |
| `check_message_state` | 19 | Message-state cases and normalized comparison |
| `logmessaged_native` | 19 | Logging peers and native process fixtures |
| `card_vehicle_source` | 18 | Vehicle oracle inputs/outputs |
| `cluster_scene_compare` | 18 | Scene rendering comparisons |
| `card_runtime_source` | 16 | Card runtime case orchestration |

Standard-library/dependency imports also show repeated infrastructure:
`subprocess` appears in 661 caller files, `hashlib` in 487, `tempfile` in 183,
and `signal` in 167. These counts overlap and must not be summed into a port
completion count.

Build one native developer-tools/QA support library with small commands, then
migrate suites in coherent families:

1. **Process and filesystem ownership.** Provide bounded stdout/stderr capture,
   process groups, deadlines, signal/exit reporting, isolated temporary roots,
   allocated local ports, child cleanup, and the existing disk reserve rule.
   Reuse suitable `process-supervision` primitives while retaining independent
   tests of the supervisor itself. Port `card_ci_space` early so every subsequent
   build command can use the native disk guard.
2. **Cases and receipts.** Provide typed JSON/JSONL inputs, deterministic output,
   source/artifact digests, source and native result comparison, and a bounded
   failure report. Preserve each suite's explicit normalization and numerical
   tolerances; do not apply a generic tolerance that masks behavior differences.
3. **IPC and persisted state peers.** Use native `params`, `msgq`, `messaging`,
   `cereal`, `can`, and `log-format` transport/data APIs to construct fixtures.
   Expected control, CAN, numerical and lifecycle results must remain independent
   of the implementation being tested. Reusing production logic to produce both
   actual and expected results would invalidate the comparison.
4. **Network and terminal fixtures.** Port async HTTP/WebSocket/upload receivers,
   deliberate disconnect/timeout peers, streamed request/response handling,
   pseudo-terminal peers and teardown. The dashcam and web suites have many
   callers of these shared behaviors, making them the first large suite family.
5. **Native build and release commands.** Port staging, checksums, ELF inspection,
   deterministic archives, release receipts and verification. Reuse compatible
   native installer/archive validation rather than implementing weaker duplicate
   checks. Preserve failed-download cleanup, exact-source identity and package
   activation bounds. The parent task owns this implementation stream.

Model compilation is a separate substantial dependency: the 24 `model_export`
files coordinate graph/kernel/export behavior and cannot be replaced by merely
invoking Python tinygrad from a Rust command. Retain explicit model provenance
and independently validated artifacts while implementing the native path. Do
not mark model developer tooling complete on the strength of a native runtime
that consumes an already compiled model.

For each family, preserve positive cases, malformed inputs, interrupted work,
resource bounds and cleanup failures. Update CI to call the native command,
verify failure detection, and only then delete the replaced maintained scripts.
If a comparison still invokes a pinned external original implementation, record
that separately; neither its source nor its interpreter may be concealed inside
the installed runtime or counted as a converted maintained tool.

## First implementation: build data extraction

The four build scripts for `carrot-server`, `dashcam-upload`, `carrot-recovery`
and `qcomgpsd` now consume maintained crate-local assets/native declarations.
They no longer read the original server configuration, recovery Python source,
or Qualcomm Python wire declarations. Asset READMEs retain the exact source
commit and original licensing provenance. Original Python sources are retained
temporarily for dependent validation tooling; deleting them is a later step.

Observed local evidence:

- Before the change, isolated native execution of all four original build
  scripts failed when their original Python source tree was absent.
- The native `source_free_builds` regression passes four cases with only
  canonical assets present, no original source tree, and an empty `PATH` during
  build-script execution. It checks that emitted asset bytes remain unchanged.
- Five extracted assets match the previous build output byte-for-byte. The GPS
  native declarations match after applying the same Rust formatting to both
  versions. All four upload default string values also equal their original
  Python literals; values were not printed or added to test output.
- Focused Cargo verification is recorded in the parent task's implementation
  ledger after the coordinated build slot completes.

Remaining old-source consumers include `carrot_server_runtime_root.py`,
`carrot_recovery_source.py`, `carrot_recovery_cwp_source.py`,
`carrot_recovery_actions_fixtures.py`, `carrot_recovery_support_source.py`,
`check_carrot_recovery_upload.py`, and the recovery transport's source-equivalence
unit test. This list is a focused finding, not an exhaustive dynamic dependency
closure. These consumers must migrate before deleting their original source.

Docs-Not-Needed: engineering inventory and implementation evidence; no setting
or public user-guide behavior changed in this extraction.

## Native package and codec tool migration

The maintained sealing, release construction, publication and encoder staging
CLIs now live in `openpilot-dev-tools`: `seal-runtime-package`,
`package-runtime-release`, `publish-runtime-release` and `stage-encoder-target`.
Their four Python implementation files, release support module and four Python
self-test files were removed after native replacement validation. Required CI
now executes the Rust package/seal/publish/encoder integration suites; its
remaining original-implementation comparisons are still explicit Python work.
The workflow isolation suite passed 24 tests and 367 subtests after replacement.

The native package suite covers malformed/incomplete inventory, duplicate JSON,
unsafe archive paths/entries, stream endings, payload tampering, static installer
identity, atomic output and the former large-file boundary. Publisher tests use
an injected GitHub transport and cover read-only checks, resumable drafts,
all-six-asset round trips, wrong identities, and publication refusal when local
assets or exact-source CI results change during network work. No real release
was published by those tests. Independent review also reproduced and corrected
requirements-type acceptance and Unicode placeholder-waiver bypasses.

The encoder tool was additionally used to download and verify the pinned native
FFmpeg/libyuv wheels and FFmpeg 7.1 source, build AArch64 libswscale with the AGNOS
SDK, and emit its receipt successfully. This validates actual native tool
execution; it is not target encoderd or recording acceptance.

`check-disk-space` provides the native build-side reserve/report CLI and shared
library. Exact reserve/growth boundaries, missing destinations, invalid CLI
arguments and integer overflow are tested. Remaining Python callers of
`card_ci_space` have not yet been migrated; its old module is consequently still
present. Native ELF auditing likewise exists and matches the original actual
AGNOS toolchain report; the remaining media validation CLI family is in progress.

The process-supervision lock-descriptor, helper-exit and controlling-PTY tests
now execute their own native Rust test executables as independent child peers.
They preserve old-protocol rejection, descendant cleanup, inherited lock
ownership, pre-handshake exit/reaping, session/group identity and controlled
versus uncontrolled terminal assertions. All five cases passed, including a
second execution with a PATH containing only `mkfifo` and `head`, with no Python
interpreter. Package Clippy passed. No production supervisor behavior changed
in this fixture migration.

The bootstrap self-test module is also replaced by eight Rust tests retaining
all 11 original cases: exact-source download, bad hash, valid installed package,
missing release, both interruption markers, network/clock and server-error
retry sequences, cancellation during download/backoff, and cached helper reuse.
The original suite passed 11 cases in 40.42 seconds before removal; the native
suite passed all eight tests in 40.15 seconds. Both retry cases still require at
least 20 seconds of actual backoff. Tests run the real bootstrap script and check
that cancellation reaps its download child and removes partial downloads. CI
now runs `--test runtime_bootstrap` alongside the native release suites.

### Native OpenCV provider tools (2026-10-14)

The three project-owned OpenCV builders and runtime stager are now native
`openpilot-dev-tools` commands: `build-xiaoge-opencv`, `build-webcam-opencv`,
`build-runtime-opencv`, and `stage-runtime-opencv`. Invoke with
`cargo run --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin NAME -- ARGS`.
The four former Python entry points were removed after migrating their current CI
calls and source-provider diagnostics. Historical investigation commands are records.
The workflow contract suite passed all 24 tests.

Preparation retains revision `fe38fc608f6acb8b68953438a62305d8318f4fcd`, archive
SHA256 `6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61`,
and the existing hardening patch and two resulting file hashes. Native extraction
rejects foreign roots, links, duplicate paths and oversized trees; GitHub's global
PAX record is metadata only. The real verified cached archive prepared successfully,
then repeat preparation verified the complete source-file manifest. Builds retain
the reviewed three/four/five-module profiles, sanitizer mode, target sysroot/provider
isolation, native libraries and upstream notices. Python bindings remain disabled;
`OPENCV_PYTHON_SKIP_DETECTION=ON` also prevents optional interpreter discovery.
External CMake/compiler/OpenCV dependencies remain explicit native providers.

The runtime stager requires pinned complete build identity, sealed file hashes,
passed ARM pixel/capture comparisons, both frozen consumers and compatible provider/
consumer ABI reports. It preflights destination conflicts and symlink parents,
preserves executable modes and aliases, and records source, validation and license
provenance. It does not stage system C/FFmpeg dependencies. Restoring an existing
provider cache is not represented as a new native build.

Validation: six focused native tests pass, including complete/idempotent staging,
failed evidence/ABI/hash/conflict/symlink-parent rejection without partial staging,
archive rejection, profile flags, and bounded-command failure records. Strict
all-target Clippy passes. Actual archive preparation and hardening ran natively;
a complete new C++ build and actual-ARM staging through this new command were not
repeated. These host checks do not establish device/runtime acceptance or CPU benefit.

### Native A630 compiler frontend and staging (2026-10-14)

`openpilot-model-runtime` now provides `offline-qcom-compile --library PINNED_SO`,
reading UTF-8 OpenCL on stdin and writing vendor binary bytes on stdout. It calls
the exact original A630 chip `0x6030001`, 64-bit compile/link ABI, checks errors,
resets failed compiler instances, and owns every compiler instance/handle/assembly.
The genuine external provider remains pinned to SHA256
`fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093`.
`stage-qcom-compiler OUTPUT ARM_FRONTEND LIBLLVM_QCOM QEMU CROSS_LIB_DIR` stages
this native frontend, checks ARM ELF architecture and complete dependency closure,
and freezes frontend/compiler/QEMU/loader identities. It replaces the ARM Python
archive and interpreter command in CI; those two Python entry points and obsolete
archive-extraction tests were removed. Host graph construction still uses Python.

Validation: two native ABI/cleanup tests and two staging admission tests pass;
strict all-target Clippy passes for model-runtime and dev-tools. The actual binary
was cross-built against the current AGNOS SDK. Real native staging passed static
per-root ABI closure; an intentionally missing actual ARM dependency rejected
before output creation. QEMU/native compilation exactly reproduced all four
checked-in original vendor binary fixtures: buffer_add (2144 bytes), constants
(2484), image_copy (2048), scalar_add (2316). Malformed OpenCL returned nonzero,
no binary stdout and the vendor diagnostic. No GPU was opened.

The remaining model-export family is not retired. Its ~2k adapter lines depend on
tinygrad Tensor/UOp, ONNX constant/shape evaluation, memory planning, scheduling,
CPU/OpenCL/AMD rendering, and the original camera warp/recurrent queue builders.
A native vendor frontend does not replace those graph transformations. Preserve
that distinction when reporting overall developer-tool conversion completeness.

Independent review tightened compiler staging before the checkpoint: each copied
input is bounded to 64 MiB, reserve uses checked actual sizes plus metadata allowance,
and file copying reads at most the reserved size plus one byte before rejecting
changes. The copied private files are audited and their frozen hashes rechecked.
Regression tests cover source replacement during audit, frozen-file mutation,
changed sizes and bounded copying. The loader hashes one bounded open descriptor
and loads that inode through `/proc/self/fd`, preventing pathname replacement
between hash and load. The ABI fixture verifies instance destruction/recreation,
success after failures, null handles, invalid tags and null instance creation.
Five native staging tests and the expanded ABI tests pass. Final AGNOS frontend
and QEMU comparisons repeated all four exact binaries and malformed-source checks.
The verified stage is `/home/fma1/rust/cache/qcom-compiler-native-296`; only the
local canonical environment's `QCOM_COMPILER_COMMAND` line was migrated to its
native `command.json`. Other environment lines and the old provider cache remain.
This environment change is local configuration, not a Git artifact.

### Native model graph foundation, not complete tensor lowering (2026-10-14)

The model-runtime export API now implements aliased base/view accounting, source
read-before-write tracking, offline initializer admission, and zero-aware packed
weights. Existing CPU graph structs serialize in original field order and retain
the runtime's validation. CPU artifact emission verifies graph/weight/library
identities and writes graph, weights, source/object files and wrapper source through
an owned temporary directory. Catalog publication validates selections, compares
entire immutable generations, and publishes a flushed pointer only after success.
It rejects duplicate selections, failed emission, corruption and extra files without
changing the current pointer. JSON preserves the original indentation, ASCII escapes,
version-1 omission and version-2 entrypoints; original Python schema/publication
fixtures compare exact graph/index/pointer bytes.

Eight focused foundation tests pass within the full 43-test model-runtime suite;
the five staging tests and strict all-target Clippy for model-runtime/dev-tools pass.
Independent source review found no concrete defect in this foundation scope. Its
publisher still requires a trusted real emitter: it does not implement tensor/ONNX
lowering, construct camera warp/recurrent policy graphs, or claim numerical inference
parity from serialization tests. The remaining Python model-export modules stay in
place. This is an intermediate native compiler API, not completed developer-tool
retirement or device acceptance.
