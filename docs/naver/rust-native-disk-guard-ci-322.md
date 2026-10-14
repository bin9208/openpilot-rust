# Native CI disk reserve guard (#322)

## Design and scope

The existing native statvfs reserve policy is extracted unchanged into the small `openpilot-disk-space` crate (rustix filesystem support, serde and serde_json). `openpilot-dev-tools` reexports its API and delegates its existing `check-disk-space` CLI. A distinct `disk-space-guard` executable avoids workspace binary-name collisions and avoids compiling the full dev-tools/native-model dependency graph during bootstrap.

179 explicit Python guard calls in `.github/workflows/rust.yml` (24 jobs) and nine in the carrot-man CI fragment become absolute `$GITHUB_WORKSPACE/rust/tools/check-disk-space` calls, preserving their paths and growth estimates byte-for-byte (verified against source 7c8b2f0f and pinned argument-sequence hashes in policy tests). This Bash/coreutils bootstrap is transparent CI glue, not another Python entry point. Some first calls precede Rust toolchain installation. The cold path checks the requested operation filesystem and the bootstrap cache, Cargo and rustup filesystems before installing the minimal pinned Rust 1.94.0 toolchain or compiling. Reserve is 25 GiB plus requested growth for the operation; bootstrap estimates one GiB. Rejection retains the instruction to recover at least 35 GiB before resuming. GNU expr performs exact integer arithmetic, avoiding Bash signed overflow.

The bootstrap builds only the tiny package with an explicit native Linux host target (x86_64 or aarch64), clears inherited cross/sanitizer flags, uses two jobs and disables incremental compilation. A source/lock/host-specific executable is atomically published under RUNNER_TEMP outside all Cargo target directories. A flock serializes cold builds; a second capacity check runs after waiting. Warm calls execute the native binary with original arguments. Changing or deleting a job target cache does not remove the guard.

## Evidence and remaining gates

Six bootstrap/policy contract tests were observed RED before implementation: no script/admission, no native dispatch, Python workflow calls, failure publication, concurrent bootstrap and operation-filesystem admission. After implementation these and five additional checks pass (eleven tests), including invalid CLI rejection before build, native arm64 selection, invalid df output and source/lock/workspace-manifest cache invalidation and interrupted-copy cleanup; fixtures intentionally do not invoke real Cargo or install a toolchain. Existing isolation policy plus these tests pass: 35 tests and 367 subtests. Bash syntax and diff checks pass.

Real native checks completed in the new isolated worktree at base `7c8b2f0f05b1140d6d391275d3643ba3daa769b1`, using canonical SDK environment, the coordinated target cache, two jobs and incremental compilation disabled. Each build passed the 25 GiB plus one GiB growth guard (over373 GiB free):

- `cargo test --offline --manifest-path rust/Cargo.toml -p openpilot-disk-space`: six tests pass (the two existing reserve/path unit tests moved from dev-tools, plus four CLI tests including non-UTF-8/nonexistent paths and both overflow stages).
- `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools`: all124 tests pass, including its existing delegated disk CLI tests.
- `cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-disk-space -p openpilot-dev-tools --all-targets -- -D warnings`: pass.
- `cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-disk-space -p openpilot-dev-tools --bins`: pass.
- Offline lock update adds only the local disk-space package and dev-tools path dependency; all pre-existing versions/checksums are unchanged.
- Actual cold x86_64 bootstrap installs/checks the pinned toolchain and compiles only the tiny dependency graph (observed compilation3.99s; not a performance claim). Its native JSON reports required26977763328 and growth134217728 bytes for128MiB growth.
- The cached executable is an x86_64 host ELF. Deleting its bootstrap Cargo build directory and changing CARGO_TARGET_DIR still permits a warm success and an impossible-growth rejection with exact JSON fields and35GiB recovery text. Cargo/rustup denial stubs prove these warm calls do not rebuild/install. No unrelated shared target cache was deleted.
- Existing `rust/tools/tests/test_card_ci.py`: five tests pass, including the retained Python per-brand capacity failure seam.

The arm64 cold path is covered by command fixtures selecting its native host target; actual aarch64 runner execution remains CI-owned. Target/device and complete runtime/tool-retirement validation are separate. Evidence and initial RED logs are ignored under `.analysis/scratch/2026-10-14-native-disk-guard-322/`.

## Subsequent raylib build guards (#326)

The original179 migrated workflow calls and their complete ordered path/growth argument baseline remain unchanged. The raylib stager migration adds five new2048MiB pre-Cargo checks, so the current workflow contains184 guard calls across26 jobs. Policy tests check the original179 argument-sequence hash independently, then validate all five new guard/native-stager pairs and preserve the original five stager argument sequences. These are179 migrations plus five new build guards, not184 migrations. The carrot-man fragment retains its nine migrated calls unchanged.

## Remaining Python inventory

`rust/tools/card_ci_space.py` is retained because `card_qa/ci.py` still imports `require_space` directly; `tests/test_card_ci.py` mocks its disk-usage seam and asserts the dict/OSError contract. `card_qa.ci` reexports this helper to stage_card_fixtures.py, stage_card_corpus.py and check_card/check_camerad/check_pandad/check_radarcan/check_ui/check_encoder CI orchestrators. Those are actual remaining Python users, not converted by changing workflow invocations. Their later family migrations must preserve the per-operation capacity checks. The new policy tests are also Python validation dependencies. This change does not claim complete development-tool or Python retirement.

## Cache admission review follow-up (2026-10-14)

Independent review reproduced a cold FIFO lock hang and unsafe local fallback warm execution. Two new contract tests observed RED before hardening. Local fallback now uses `${XDG_CACHE_HOME:-$HOME/.cache}`; CI retains RUNNER_TEMP. Before warm execution or build, existing cache parent/base/key directory, lock and executable must have the current UID, expected type, no symlink and no group/world write bits. Unsafe objects fail untouched; no chmod/chown repairs somebody else's object. Creation uses umask077, directories700 and published executable700. Cold lock opens RDWR (no FIFO partner wait), validates opened-FD regular-file ownership/mode before flock, and rechecks cache admission after acquiring it.

Policy37 tests plus367 subtests passed, including unsafe base/key/binary/lock modes, symlinks, controlled wrong-owner metadata, FIFO lock, private fallback, secure warm reuse, parallel publication and interrupted publication cleanup. FIFO failures are bounded by an owned3s test deadline. Actual fresh private-cache native host bootstrap compiled only the tiny disk-space crate/dependencies in3.97s after reserve admission (397GB free); actual warm invocation emitted the same native JSON without another build. These tests do not claim resistance to arbitrary same-UID concurrent filesystem mutation or privileged tampering.


Final CI routing audit found that the inherited unittest discovery imports but
does not execute the new pytest-style bootstrap tests. Fast checks now invoke
that file explicitly through pytest, with repository conftest excluded just
as in the other focused CI suites. The exact CI invocation passes all13 tests;
the broader37/367 local policy result above includes those same13 tests.

## ARM UI workflow test follow-up (2026-10-15)

The complete `rust/tools/tests` suite exposed a stale assertion in
`test_arm_ui_commands_resolve_workspace_and_python_paths`: it required Python
commands in the ARM UI build step because the original four capacity checks
invoked `card_ci_space.py`. The native migration correctly removed those
commands. Before repair, the complete suite reported195 passed/1 failed,
9 passing subtests, and no skips.

The test retains its effective working-directory/Cargo workspace-membership
checks and validates any remaining Python script paths. It now requires exactly
four native guard argument lists, with absolute GITHUB_WORKSPACE bootstrap/path
resolution and ordered growth values2048,256,2048,256 MiB. Each resolved
executable must be an existing absolute file. Controlled missing-guard,
relative-path, changed-growth and legacy-Python substitutions all fail the
updated assertion; no production workflow or capacity policy changed.

Focused UI tests:5 PASS. The complete suite then passed196 tests plus9 subtests,
zero skips, in50.28s. Six inherited NumPy shape-assignment deprecation warnings
remain. Execution used Python3.12.15, pytest9.0.2, numpy2.5.3, onnx1.20.1,
pycapnp2.1.0 and pyzmq27.2.0; current-worktree source overrides and existing
native model-run/qcom-model-run binaries. The cached ARM offline compiler used
the pinned library SHA256
`fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093`,
verified against its provenance and cached command. No Cargo build, install or
device action was needed. Ignored logs/JUnit are under
`.analysis/scratch/2026-10-15-python-workspace/`; exact final-SHA cloud CI remains
a separate gate.
