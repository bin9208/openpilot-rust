# Updater remote regression first (#419)

Issue: https://github.com/bin9208/openpilot-rust/issues/419

Base: `dc95138076836ea3a5c1b6c27262be3f3b912f71` (`origin/dev-md`).
Branch: `codex/fix-419-update-remote`. The first commit added regressions against
unchanged production code. After confirmed cloud RED, the next scoped change
implements selection and updates the affected fixtures, checker and user guides.

## Required behavior and coverage

Select the **current** branch's configured `branch.<name>.remote`. Fall back
to `origin` only when that setting is absent. Use that named remote for the
HEAD probe, explicit head discovery, fetch/refspec, and target upstream.
Preserve supported explicitly configured HTTPS/HTTP/SSH/Git repositories and reject
missing, empty, dot, local, and unsupported remotes before remote I/O.
No new Params or UI are proposed.

`rust/crates/updated/tests/update_remote.rs` uses the existing public
`Updater`/`Commands` seam. Real bare Git repositories have different main
commits; only rust has dev-md and preview. Only transport commands receive
per-command URL substitution to the owned repositories. Metadata/config
reads see supported URLs; Git actually advertises, fetches, checks out,
sets upstream, and finalizes a small synthetic checkout. The seam blocks
network transports and never invokes installation or OS operations.

Cases cover explicit rust discovery/probing, fetching when origin lacks
dev-md, ordinary origin, absent-setting fallback, a renamed remote with
HTTPS and both SSH URL forms, a requested target with conflicting remote
config, and branch switching within one updater instance. Invalid-source
controls check both discovery and fetch rejection, unchanged checkout/config,
and preservation of an already-ready update.

## Cloud RED confirmed; focused GREEN pending

Regression commit: `027d676a9215159c978096090bfe349e3249783e`.
The parent confirmed [Actions run 39823345157](https://github.com/bin9208/openpilot-rust/actions/runs/39823345157)
completed strict affected lint successfully, compiled the 14-test regression
suite, and failed on the real updater assertions/fetch behavior. This is the
required RED gate; it is not a compilation or infrastructure failure.

The parent will push the implementation and inspect the exact-SHA result of
`Updater remote regression 419`. Its only trigger remains a relevant push to
this issue branch. It runs on `ubuntu-24.04` with pinned Rust 1.94.0,
bounded build jobs, incremental compilation disabled, and the repository
disk guard before dependency/build growth. Commands:

```sh
cargo fmt -p openpilot-updated --check
cargo clippy -p openpilot-updated --all-targets --locked -- -D warnings
cargo test -p openpilot-updated --all-targets --locked -- --nocapture
python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=tools/tests tools/tests/test_updated_comparison.py tools/tests/test_updated_remote_fixture.py -q
```

The workflow also builds the affected updater/process-supervision entrypoints
and the existing original Params binding, then runs `check_updated.py` against
the actual original/native updater traces. Local verification is limited to Rust
format/syntax and diff review; no tests or Git fixtures run locally.
No push, dispatch, issue/PR mutation, or device action is part of this delegated
step. `ci.yml` and its required PR gates remain unchanged.

## Implementation and compatibility boundary

Selection is recomputed from the current staging branch before each check or
fetch. Only Git config exit 1 permits origin fallback; an explicitly empty,
missing, dot, option-like or otherwise unsupported remote fails. Effective URL
validation uses `git remote get-url`, including `insteadOf` expansion. URL parsing
uses the already-locked `url` crate; file paths, remote helpers and unsupported
schemes are rejected before remote I/O or config/checkout changes. HTTPS, HTTP,
SSH, Git URLs and scp-style SSH URLs remain supported.

For the original checker, the origin-only fixture begins on dev. The expected
ordered source command list gains exactly three reads at each selection boundary:
`rev-parse --abbrev-ref HEAD`, `config --get branch.<current>.remote`, and
`remote get-url origin`. Its implicit `ls-remote --heads` becomes explicit origin.
Checkout commands advance the expected current branch, and a new fixture overlay
mount resets it to dev. Existing staging-root removal compatibility is retained.
Native commands are not filtered: snapshots, Params, AGNOS calls, cwd, all other
commands, and the complete expanded ordering remain strictly equal. Focused
checker tests reject missing, extra, reordered, mis-scoped or changed commands.

The source fixture now stores a supported HTTPS literal. A PATH-controlled Git
transport wrapper maps only ls-remote/fetch to the owned local repository;
metadata and all actual Git assertions remain intact. The existing failure-23
test now supplies real Git metadata and preserves ready-update assertions.
Additional cases reject local/unsupported URL mappings and unsafe remote names.
Paired Korean/English rust-port guides describe the changed behavior.

Focused GREEN, the existing full PR pipeline against dev-md, and mapped-doc
validation remain pending cloud execution. The parent owns pushing, exact full
PR validation and AGNOS candidate delivery; device acceptance remains user-owned.
Remove the temporary workflow when its purpose is complete.

## Delivery maintenance patch, prepared locally on 2026-10-23

The parent owns PR #499 to dev-md and all remote orchestration. This patch is
prepared on `6a5143b30dbc365e056de43518cfc97bcf16681f`; no commit, push, dispatch,
PR edit, or cancellation is authorized at this stage. Existing CI 39829425141
continues unchanged. Device installation and acceptance remain user-owned.

Plan and review boundary:

- Add exact reviewed updater/host-admission paths and positive/neighbor-negative
  tests at the actual `source_path_allowed` consumer, without new provider prefixes.
- Adapt only the shared host growth policy and its capacity tests from the
  backlog reference `7acb65c12af5194e741dd1a1ea47769c0d31404d` (native/Bash changes
  originate in `77100d33ce4d1202853bd6bf3597a0fc2014c590`). No #296 tools conversion,
  provider builder, device-space policy or runtime algorithm is imported.
- Route only verified lightweight x64 work and validated AGNOS delivery to
  registered bin-actions; preserve the serial graph, required five check names,
  all tests and architecture conditions. Keep heavy/memory/matrix jobs hosted.
- Review syntax/format/diff locally; leave every functional/build/install recipe
  for Actions and hand off this uncommitted patch for parent review.

### Actual source baseline and exact admission

The delivery driver uses `BASE=3496c4bbf300e7487d17b2bf25afc170584dfe02`, not
the PR branch point. Its actual reviewed path inventory was read with:

```sh
git diff --no-renames --name-only 3496c4bbf300e7487d17b2bf25afc170584dfe02 6a5143b30dbc365e056de43518cfc97bcf16681f
git diff --name-only dc9513807 6a5143b30dbc365e056de43518cfc97bcf16681f
git diff dc9513807 6a5143b30dbc365e056de43518cfc97bcf16681f -- rust/Cargo.lock
```

The #419 delta has thirteen paths. Cargo.lock was already admitted. The twelve
missing exact entries are:

```text
.github/workflows/update-remote-419.yml
docs/naver/dev-md-update-remote-419.md
docs/user/en/rust-port.md
docs/user/ko/rust-port.md
rust/crates/updated/Cargo.toml
rust/crates/updated/src/updater.rs
rust/crates/updated/tests/runtime_package.rs
rust/crates/updated/tests/update_remote.rs
rust/tools/check_updated.py
rust/tools/tests/test_updated_comparison.py
rust/tools/tests/test_updated_remote_fixture.py
rust/tools/updated_fixtures.py
```

The neutral host admission changes need six further exact entries:

```text
rust/crates/disk-space/src/lib.rs
rust/crates/disk-space/tests/cli.rs
rust/crates/dev-tools/src/package.rs
rust/crates/dev-tools/tests/disk_space.rs
rust/tools/check-disk-space
rust/tools/card_ci_space.py
```

The dev-tools package source change is confined to shared-space-guard unit tests;
packaging/sealing algorithms are unchanged. Other modified host workflow/test
paths were already admitted individually or by existing action-directory rules.
New policy tests exercise the actual `git diff --no-renames BASE HEAD` including
the existing identical-tree-entry requirement for inherited runner documentation.
They reject neighboring updater modules, build.rs, unrelated crates/builders,
file suffixes, children, models and native provider sources.

The only #419 lock change is the updated package's direct dependency edge to
`url`. Version 2.5.8 and its existing checksum/dependency closure were already
locked; no external provider version/source changes are introduced. The updater
change is pure Rust. The driver still cleans/rebuilds every selected project
package against the exact merged source, rebuilds the already-required OpenCV
provider, audits AGNOS ABI closure, seals, and verifies offline/online installation.
Provider-reuse fingerprints, hardening receipts and negative boundaries remain.

### Runner and space scope

The parent reports actual AGNOS comparison success at source b538…: bin-actions
35m47s versus GitHub-hosted 44m47s. Completed job 119987118119 records Ubuntu
24.04 Noble and clang/libclang 18. This patch does not assume Ubuntu 22.04.
Generic GNU/musl comparison 39832976197 remains owned elsewhere and pending;
that generic cross-build stays on GitHub-hosted x64. Actual ARM CPU jobs stay on
GitHub ARM; all logical matrix runner values and conditions remain unchanged.

Only `fast-checks`, `check_user_docs`, `git_update`, `model_web`, `jetlink-native`
and the focused updater job move to bin-actions for trusted repository work.
Forks/fork PRs retain hosted fallback. API-only controls and unverified deep,
workspace, provider, memory and matrix jobs keep existing routing. The validated
AGNOS package and publish/online-QEMU-validation jobs use
`[self-hosted, Linux, X64, dev-md-local]` after exact-source verification.
The driver rejects other self-hosted identities and still requires GitHub Actions.
Hosted tool-bundle removal is guarded before sudo on persistent runners.

Host capacity admission requires the estimated operation growth, without the
retired 25/35 GiB reserve. Existing growth estimates and bootstrap ownership,
hashing, cache isolation and overflow checks are retained. Package room checks
append free/growth observations and actual runner identity/distribution; installer
metadata names the actual runner rather than asserting GitHub-hosted execution.
Device installer space policy and unrelated heavy QA helpers are unchanged.

### Frozen local patch paths

```text
.github/actions/rust-path/action.yml
.github/workflows/ci.yml
.github/workflows/dev-md-runtime.yml
.github/workflows/update-remote-419.yml
AGENTS.md
docs/naver/dev-md-update-remote-419.md
rust/ci/runtime/package.py
rust/ci/runtime/test_package_policy.py
rust/crates/dev-tools/src/package.rs
rust/crates/dev-tools/tests/disk_space.rs
rust/crates/disk-space/src/lib.rs
rust/crates/disk-space/tests/cli.rs
rust/tools/card_ci_space.py
rust/tools/check-disk-space
rust/tools/tests/test_card_ci.py
tools/ci/tests/test_native_disk_guard.py
tools/ci/tests/test_rust_path.py
tools/ci/tests/test_workflow_policy.py
```

### Actions-only checks after parent review

```sh
python3 rust/ci/runtime/test_package_policy.py
python -m unittest discover -s tools/ci/tests -v
python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=tools/ci/tests tools/ci/tests/test_native_disk_guard.py -q
PYTHONPATH="$PWD/rust/tools${PYTHONPATH:+:$PYTHONPATH}" python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=rust/tools/tests rust/tools/tests/test_card_ci.py -q
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-disk-space
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --lib package::tests::disk_growth
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --test disk_space
```

Then the existing full serial PR gates remain required. After a marked dev-md
merge and successful exact-source push CI, the unchanged delivery sequence is
`package.py source-policy`, `prepare`, `host-tools`, `build`, `publish`, all within
the verified Actions workflow. No local execution of these commands is permitted.
Runner cache behavior, actual source-policy admission and operation estimates
remain CI risks until this new patch's exact revision is tested. The running
checks are neither cancelled nor superseded by this uncommitted preparation.

The next branch-focused job also fetches full source history and runs the actual
package-BASE admission policy plus native host-growth and compatibility-wrapper
tests before updater validation. This catches delivery/guard regressions without
waiting for the deep serial gate; it does not replace that gate.
