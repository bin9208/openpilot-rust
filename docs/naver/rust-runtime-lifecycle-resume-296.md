# Lifecycle handoff follow-up (#296)

Scope: resumed lifecycle/package-integrity work for #287, #290, #297 and #309.
The prior worker's host evidence is in `rust-runtime-module-audit-296.md`.
No prior implementation or unrelated source was discarded.

Review confirmed the atomic source exchange reexecutes the installed new launcher,
keeps the old checkout as a rollback, and requires a verified matching package.
Live generation cleanup retains whole owner generations and helper siblings;
old-owner retirement rechecks executable/start identity through pidfds, without
an unsafe raw-PID fallback on old kernels. These are source-review conclusions,
not new device acceptance or an all-module clearance.

Two additional transaction regressions are under investigation:

- A local Git clone invents origin branches from active local branch heads,
  dropping actual upstream-only refs and the original upstream default.
- Implicit staging rewrites an existing selected branch's upstream to its own
  local name. A local branch tracking `origin/release` could subsequently follow
  another upstream branch after activation.

The new tests are in `runtime-installer/tests/cli.rs`. Verification and the
minimal correction will be recorded here after the coordinated host Cargo slot.

Docs-Not-Needed: internal engineering lifecycle evidence; no setting changes.

## Verified correction

Both new tests first failed against the inherited transaction: the real
`origin/remote-only` ref was missing (local `candidate` became an origin branch),
and implicit upstream `refs/heads/release` became `refs/heads/master`.

`transaction.rs` now replaces the clone-invented origin namespace with the
source checkout's actual fetched origin refs, including its symbolic default.
The approved canonical origin URL and normal fetch namespace remain pinned.
Implicit updates preserve the active branch's configured origin merge ref;
explicit branch selection continues to track its explicitly selected name.
Existing local branches and independent Git objects remain preserved.
This does not copy arbitrary Git configuration or hooks into the candidate.

The initial full installer run also reproduced Linux ETXTBSY in
`activation::tests::running_owner_retains_sibling_helpers_until_exit`: a parallel
subprocess can inherit `fs::copy`'s writable descriptor before exec closes it.
The fixture now retries only that error for at most two seconds, matching the
existing native process fixture policy; no production ownership rule changed.

Final validation with canonical environment, shared target cache, two jobs and
incremental compilation disabled:

- `cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-runtime-installer`:
  all eight unit and 45 CLI integration tests passed.
- `cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-runtime-installer --all-targets -- -D warnings` passed.
- Focused Rustfmt and `git diff --check` passed.
- Disk checks before builds retained over 381 GiB available against 25 GiB plus
  512 MiB estimated growth.

These results apply to uncommitted source changes on the existing #296 branch.
The parent must rebuild AGNOS installer and its linked launcher consumers after
this transaction change. No commit, push, device connection, package publication,
normal boot or vehicle acceptance was performed by this worker. Previously
recorded original/native comparisons remain prior-worker evidence rather than
newly rerun claims.
