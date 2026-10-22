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
