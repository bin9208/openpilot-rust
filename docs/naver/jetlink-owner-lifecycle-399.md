# Jetlink owner lifecycle — issue #399

Source: [bin9208/openpilot-rust#399](https://github.com/bin9208/openpilot-rust/issues/399), in the isolated #296 repository-conversion branch at baseline `11f3dec411662baa961733b363c0b38d7082fcfa`.

`Server::stop` previously removed its socket pathname even when `backend.connect` or `UnixListener::bind` failed. It also wrote worker diagnostics to stderr during stop/drop; a full stderr pipe could block shutdown beyond the supplied timeout. The worker-completion path could likewise write to a closed receiver's stderr. The fix records a held `O_PATH | O_NOFOLLOW` filesystem inode after bind, then removes only a still-matching socket pathname after the worker joins. Worker failures remain in `Server.error`; stop retains its existing `Result` behavior. No USB, model or device operations are part of the regression tests.

The owner process lock excludes another legitimate binder while the pathname is opened. The socket directory must also be trusted against concurrent replacement: bind-to-`O_PATH`, pathname permission changes, and compare-to-unlink are separate syscalls, not an atomic protection against a hostile actor changing the path at those instants. The held inode prevents reuse from making a later replacement appear owned.

Run RED on a temporary branch at baseline `11f3dec4` with only `rust/crates/jetlink/tests/owner_lifecycle.rs` added. Each command targets one assertion so the first failure cannot mask another:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --test owner_lifecycle failed_connect_preserves_foreign_listener -- --exact --nocapture
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --test owner_lifecycle failed_bind_preserves_foreign_listener -- --exact --nocapture
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --test owner_lifecycle ready_owner_cleanup_preserves_a_replacement_listener -- --exact --nocapture
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --test owner_lifecycle full_stderr_cannot_extend_failed_stop -- --exact --nocapture
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --test owner_lifecycle full_stderr_cannot_extend_drop -- --exact --nocapture
```

Run GREEN on the integrated fix SHA in GitHub-hosted Actions:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-jetlink --test owner_lifecycle -- --nocapture
```

As of 2026-10-21, only local formatting, syntax parsing and diff checks have been performed. Record the exact fix SHA, Actions URL, RED/GREEN outcomes and any failures here after cloud execution. Do not infer device startup or USB behavior from these tests.
