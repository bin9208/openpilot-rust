# Updated host validation

Issue: [#118](https://github.com/bin9208/openpilot-rust/issues/118).
Scope and native/unported boundaries: [implementation note](../naver/rust-updated-118.md).

Use Rust 1.94, a shared bounded target cache and two build jobs. Check disk space
before builds and preserve the repository's free-space floor. The Python source
oracle needs the existing original compiled Params binding, numpy and pyzmq; the
command/source fixtures otherwise use the standard library. All Git repositories,
files, command groups and IPC endpoints are owned by the scenario.

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-updated -p openpilot-process-supervision --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml -p openpilot-updated -p openpilot-process-supervision --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-updated --bins --examples --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-process-supervision --bin openpilot-process-child --locked
PYTHONPATH=. python rust/tools/check_updated.py --binary "$CARGO_TARGET_DIR/debug/examples/updated_trace" --launcher "$CARGO_TARGET_DIR/debug/openpilot-process-child" --binding "$PARAMS_BINDING" --output "$EVIDENCE/source"
PYTHONPATH=. python rust/tools/check_updated_process.py --binary "$CARGO_TARGET_DIR/debug/examples/updated_command" --launcher "$CARGO_TARGET_DIR/debug/openpilot-process-child" --output "$EVIDENCE/process"
PYTHONPATH=. python rust/tools/check_updated_daemon.py --binary "$CARGO_TARGET_DIR/debug/openpilot-updated" --launcher "$CARGO_TARGET_DIR/debug/openpilot-process-child" --output "$EVIDENCE/daemon"
PYTHONPATH=. python rust/tools/check_updated_agnos.py --target "$CARGO_TARGET_DIR" --output "$EVIDENCE/agnos"
```

The source comparison executes the unchanged updater `main`/Updater/function
bodies with an original Params binding, controlled UTC clock, build-channel and hardware/AGNOS
boundaries, actual temporary Git repositories and a PATH-selected overlay
fixture. It compares raw Params bytes, ordered commands, branches, internet
state, consistency markers and sleep decisions. Successful-fetch scenarios
also require an actual finalized consistent checkout; matching failures cannot
pass. Cases cover check/fetch/metered timeout, missing branch and aware-time
failure, tizi mapping, AGNOS manifest selection, connectivity thresholds and
invalid integer counter fallback. The environment lacks Git LFS locally; its
failed prune is intentionally nonfatal as in the source, while invocation order
is checked. This is not physical OverlayFS or LFS-data validation.

The process comparison exercises source/native merged output, nonzero exit,
existing command-scope Git settings and appended maintenance overrides. Two
owned Git-style descendant scenarios ignore SIGINT/SIGTERM and hold the output
pipe with a live or exited direct parent. Both source and native helper must
remove the descendant within the manager's stop window after SIGINT.

The production-entrypoint scenario verifies lock exclusivity, SIGUSR1 check,
SIGHUP fetch/finalization, persisted descriptions/notes, symlink/executable mode,
actual logging and clean SIGTERM exit. Git operates on owned repositories; sudo
and OverlayFS operations are replaced only at the external command boundary.
The unit tests cover note rendering/fallback, filesystem copying/flags, remote
ref filtering, command environment, request retention and typed stored dates.

The receipt and captures live under `.omo/evidence/updated-118-*` in the main
checkout. The linked background adapter adds source/native comparisons through
actual harmless `abctl` children and owned regular-file partitions: compressed
images, background casync, and corrupt-image failure, including ordered slot
commands and exact file digests. Source imports require the same Python packages
as the AGNOS gate. Cloud workspace/aarch64 checks remain parent gates.
No CPU savings, physical AGNOS installation, device acceptance or complete-runtime readiness
is inferred from these host results.
