# Repository-wide Python retirement implementation plan (#296)

> **For agentic workers:** Use `superpowers:executing-plans` for parent-owned tasks and bounded independent workers for the named families. This plan is an execution ledger, not evidence of completion.

**Goal:** Replace all maintained Python functionality in the repository, including development, build, test, model compilation and vendored tooling, with verified native Rust equivalents; remove replaced Python source only after its callers and regression coverage have migrated.

**Architecture:** Preserve independent original outputs before removing their implementation. Reuse native runtime and developer-tool libraries, port complete caller/dependency families, and verify each command's successful outputs and failure effects. A Rust command invoking Python, relocated source, downloaded old source, or precompiled model consumption alone does not replace the corresponding compiler/tool functionality.

**Tech Stack:** Rust 1.94.0, the locked Cargo workspace, native compiler/provider boundaries, existing synthetic protocol fixtures and original-source comparison evidence.

**Spec:** `docs/rust-port/design.md`, `docs/naver/rust-python-tools-retirement-296.md`, and the user's 2026-10-15 repository-wide clarification. Tracking: #296 and #1.

## Global constraints

- Base: stable dev `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`; work branch `codex/feat-296-repository-python-retirement`, targeting dev through required checks.
- The concurrent `dev-md`/#338 track and its dirty primary checkout are separate. Never merge, cherry-pick, reset, stage or edit its work. This plan does not promote either track to main.
- Reuse `/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust`; preserve prior branches and current runtime release evidence.
- Repository-wide means current tracked source, including vendored Python, Cython, SCons and executable embedded Python. Git history and licensing/provenance remain intact. The operating system's independent installed software is outside repository deletion.
- Preserve existing functionality, failure/cleanup behavior and test assertions. No weakened tolerances, cadence, validity limits, skipped suites or self-generated expected answers.
- Count every remaining source/dependency explicitly. Native runtime success does not complete development tools, compiler functionality, Python removal or device acceptance.
- Before builds, installs and large copies, retain 25 GiB plus estimated growth; recover at least35 GiB if below the floor. One coordinated Cargo owner, two build jobs, incremental disabled. Do not interrupt another track's build.
- All separately launched workers use GPT-6.1 Sol. Parent owns shared Cargo manifests, workflows, deletion integration and Git publication.
- On2026-10-18 the user confirmed the device is unavailable and will personally pull and test normal boot, recording and upload. Finish code/test/commit integration without waiting for a connection; user device results remain separate evidence.
- Preserve the five required PR checks and separate exact merged-dev PUSH checks. No completion merely from opening a PR or scheduling CI.

## Baseline and remaining scope

The base has3,542 tracked `.py` files and38 additional Cython/SCons files. Initial `.py` locations: rust/tools1238; openpilot1017; tinygrad_repo753; opendbc_repo272; other262. These counts are inventory, not semantic equivalence or completed conversion.

Every directory remains in scope. In particular, tinygrad's tensor/compiler, autograd/training, optimizers, datasets, examples, tests and developer utilities cannot be discarded merely because deployed models run natively. Original Python runtime implementations still used by comparison tools must retain their regression value through independent fixtures/native comparison machinery before deletion.

## Review focus

1. Removed source still reached through a workflow, dynamic import, generated script or subprocess: exercise actual callers and audit execution without an interpreter.
2. Native validation accepting a case the original rejected: retain malformed input, partial output, timeout, signal and cleanup negative controls.
3. Expected answers derived from the replacement itself: preserve independent original captures, provenance and deliberate corruption/mutation failures.
4. Source classification hiding vendored, extensionless or embedded Python: inventory the full tracked tree and actual build/test entrypoints, not only selected directories.
5. Concurrent dev-md changes leaking into this branch or shared build artifacts: preserve isolated source roots, explicit branch identities, coordinated builds and exact-source receipts.

## Current implementation units

### Task1: native CI policy and bootstrap regression family

Files: new `rust/crates/dev-tools/src/ci_policy.rs`, `tests/ci_workflow_policy.rs`, `tests/ci_rust_isolation.rs`, `tests/ci_disk_bootstrap.rs`, `tests/ci_process_probe.rs` and focused support files. Parent integrates dependency/module registration and `.github/workflows/fast-checks.yml`.

Interfaces: a native YAML access layer preserving the original BaseLoader scalar meaning; policy validation against supplied workflow trees; real shell aggregator/bootstrap execution with owned temporary roots.

- [x] Preserve every original test/assertion and its input; write native baseline and independent mutation negatives first.
- [x] Observe RED for missing native behavior; implement the smallest native equivalent, retaining string keys such as `on`, scalar spelling, matrices, guards and failure states.
- [x] Run native tests and original policy suite; resolve differences without dropping checks.
- [x] Replace the CI test invocation and remove the three policy/isolation/bootstrap test files only when all corresponding cases are preserved. Retain the actual Python process-probe regressions until its active comparator is replaced; tests of a new unused native helper do not replace those regressions.
- [ ] Retain `check_changed_files.py` explicitly until equivalent Python syntax admission exists or final Python retirement makes that branch obsolete. Do not replace `ast.parse` with a lexical check.

### Task2: native Panda source-oracle build commands

Files: new `dev-tools/src/panda_oracle_build.rs`, five `src/bin/build_pandad_*.rs` entries and `tests/panda_oracle_build.rs`; parent updates `check_pandad_ci.py` callers and manifest registrations.

Interface: protocol (all five modes), USB, SPI, peripheral and runtime-fixture builders with existing output/prefix/compiler/sanitizer options and native command execution. Their output remains the same C++ oracle/fixture artifacts.

- [x] Capture independent original commands, generated includes and manifests across all modes and sanitizer variants.
- [x] Observe failing native tests before implementing builders. Preserve compiler ordering, extraction failures, disk guards, partial-output/failure evidence and absence of a false success manifest.
- [x] Compare generated bytes and normalized command/manifest outputs; build real C++ artifacts and run existing equivalence cases.
- [x] Wire native commands, verify all callers and remove the five replaced Python builders.
- [ ] Keep remaining Panda/runtime-source/Cython helpers explicitly pending until their own native replacements pass.

Additional locally verified replacements and current failures are recorded in the
[evidence ledger](rust-repository-python-retirement-296-evidence.md) and inventory.
96 baseline sources are fully replaced locally, five fixed checker CLIs are
partially replaced because their generic live-source helpers remain Python,
and3,479 baseline sources remain pending. The added
rust/tools/selfdrived_helpers_source.py retains those helpers; relocation is
not retirement. Remaining families and final delivery gates stay open.

### Task3: native model compiler operations, then the full compiler chain

Files: initial `model-runtime/src/compiler_launch.rs`, `tests/compiler_launch.rs`; subsequent units cover the exact remaining `rust/tools/model_export/` and tinygrad dependency families.

Interface: launch geometry takes original dimension types, optional local dimensions, OpenCL source and genuine compiled kernel bytes; returns the same groups/local dimensions or rejection. This is one compiler operation, not a complete compiler.

- [ ] Preserve the original launch/body results and binary provenance; observe RED and implement equivalent bounds, registers, divisibility and work-item coverage.
- [ ] Compare with genuine compiler artifacts and preserve all original rejection cases before replacing the Python operation/tests.
- [ ] Implement native ONNX/protobuf admission, metadata/slice decoding, tensor IR, operators, scheduling, memory planning, layout transforms, code generation and native provider compilation.
- [ ] Implement complete CPU/QCOM/AMD export, preparation/warp/history/recurrent semantics and immutable publication; test actual model outputs and error behavior against independent original evidence.
- [ ] Replace remaining tinygrad training/autograd/optimizer/dataset/example/test/development functionality with its own coverage. Record concrete units before implementation; none is implicitly exempt.

### Task4: remaining repository families and final deletion gate

Files: complete tracked source/dependency inventory; per-family native replacements and tests across rust/tools, tools, scripts, release, openpilot, opendbc_repo, panda, msgq_repo, rednose_repo, teleoprtc_repo and other vendored sources.

- [ ] Map each tracked Python/Cython/SCons file to actual callers, native replacement, retained behavior and verification evidence. Pending entries remain pending even if the runtime no longer selects them.
- [ ] Convert shared QA/process/IPC/fixture infrastructure and all remaining comparison suites; migrate original runtime-source dependencies after preserving independent expectations.
- [ ] Convert all remaining build/generation/documentation/navigation/Jetlink/log-analysis/replay/developer commands and their tests, preserving supported interfaces.
- [ ] Remove each replaced dependency closure only after real callers, success/error cases and native coverage pass. Preserve required licensing/source provenance as data/documentation.
- [ ] Verify zero tracked Python/Cython/SCons/extensionless Python implementations and no hidden/downloaded Python execution in maintained build/test/runtime entrypoints; run complete native suites without a Python interpreter available.
- [ ] Pass independent branch review, exact PR and merged-dev gates and final source-bound runtime packaging. Deliver the verified code for the user's requested pull/test; record boot/record/upload results when the user supplies them. Report repository-wide conversion complete only when every source-retirement requirement is met.

Remaining large units are decomposed from inspected contracts as their prerequisite families become available. Completing Tasks1–3 alone does not complete Task4 or this project.

Docs-Not-Needed: internal engineering plan; no user setting changes.

### Follow-up compiler unit: pinned native LLVM provider and IMAGE scheduling

Additional independent IMAGE controls found output-layout-dependent signed-zero
behavior in the original LLVM optimization pipeline. Original emitted IR starts
at +0 and uses `nsz arcp contract afn`; unrolling/SLP sometimes removes that initial
zero and sometimes retains it. A channel/count-based C accumulator shortcut is
not equivalent and remains uncommitted. Existing positive tests stay intact.

- [ ] Add an isolated `offline-llvm-compile` native worker using the existing
  libloading dependency, explicit library SHA, target triple/CPU/features,
  pass pipeline and unroll/vectorization/SLP/verification configuration.
- [ ] Verify module admission, per-instance ownership/cleanup, bounded child
  execution, failed-output cleanup and source/object/optimized-IR receipts against
  independent original CPULLVMCompiler results for configured LLVM18 and20.1.
  Same-version/context equality is required; cross-version equality is not claimed.
- [ ] Port parameterized IMAGE packing and reduction/output loop scheduling from
  the retained source algorithms, preserving four-lane layout, padding/undo and
  initial +0. Emit LLVM with the original flags and let the configured optimizer
  make its decisions; do not select behavior from observed channel/width lists.
- [ ] Integrate objects through the existing artifact ABI only after reference
  cases, earlier positive coverage, actual model prefixes and full model checks
  pass. Keep generic normalization, other schedulers and full compiler removal
  explicitly pending. Wire mandatory CI provider validation before publication.

### Follow-up UI units: source polygon reference and image comparison

The independent Rust polygon reference uses a narrow raw raylib ABI and preserves
editable scenes, frame order, shader policy, state artifacts and failure phases.
Actual original/native host PNG/state comparisons are intermediate evidence;
source-helper retirement still requires review repairs and caller migration.

A separate decoder unit preserves `PIL.Image.open`/NumPy raw modes, dimensions,
palette indexes, integer conversion and broadcasting. The original checker accepts
non-PNG content under `.png` names, including TIFF/TGA/DDS/GIF/SGI/IM, and some
palette/16-bit broadcasting inputs. RGBA normalization or PNG-only decoding does
not preserve those admitted cases. Independent codec arrays and comparisons must
close proven gaps before retiring the checker. No Python wrapper or discarded
format coverage counts as a native replacement.
