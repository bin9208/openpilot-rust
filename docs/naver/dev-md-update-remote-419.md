# Updater remote regression first (#419)

Issue: https://github.com/bin9208/openpilot-rust/issues/419

Base: `dc95138076836ea3a5c1b6c27262be3f3b912f71` (`origin/dev-md`).
Branch: `codex/fix-419-update-remote`. This step changes tests and a temporary
branch-only GitHub-hosted workflow; production updater code is unchanged.

## Required behavior and coverage

Select the **current** branch's configured `branch.<name>.remote`. Fall back
to `origin` only when that setting is absent. Use that named remote for the
HEAD probe, explicit head discovery, fetch/refspec, and target upstream.
Preserve supported explicitly configured HTTPS/SSH repositories and reject
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

## Cloud RED gate pending

The parent will push this commit and inspect the exact-SHA result of
`Updater remote regression 419`. Its only trigger is a relevant push to
this issue branch. It runs on `ubuntu-24.04` with pinned Rust 1.94.0,
bounded build jobs, incremental compilation disabled, and the repository
disk guard before dependency/build growth. Commands:

```sh
cargo fmt -p openpilot-updated --check
cargo clippy -p openpilot-updated --all-targets --locked -- -D warnings
cargo test -p openpilot-updated --locked --test update_remote -- --nocapture
```

Expected RED must be assertion/fetch failures caused by unchanged updater
remote selection, after successful compilation. Compilation or infrastructure
failure alone is not RED evidence. Local verification is limited to Rust
format/syntax and diff review; no tests or Git fixtures run locally.
No push, dispatch, issue/PR mutation, device action, or production fix is part
of this step. `ci.yml` and its required PR gates remain unchanged.

## Follow-up only after observed cloud RED

- Implement the selected-remote policy without caching across branch switches.
- Keep `check_updated.py::assert_equivalent` strict for snapshots and unrelated
  commands. Add only a targeted, tested compatibility allowance for remote
  metadata reads and explicitly selected commands in `updated_trace` output.
- Existing `updated_fixtures.py` seeds local-path clone URLs; adapt its transport
  boundary to supported configured URLs without weakening local-remote policy.
- Replace the empty responses in the existing failed-fetch command fixture with
  supported Git metadata, retaining failure code 23 and ready-marker semantics.
- Include paired `docs/user/ko/rust-port.md` and `docs/user/en/rust-port.md`
  updates when implementing the user-visible behavior.
- Observe focused GREEN and the full existing PR pipeline against dev-md.
  Remove the temporary workflow when its purpose is complete. Exact-source
  AGNOS delivery and user device acceptance remain later gates for #419.
