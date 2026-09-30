# Hardwared host validation

Issue: [#111](https://github.com/bin9208/openpilot-rust/issues/111).
Implementation and remaining boundaries: [runtime note](../naver/rust-hardwared-111.md).

Run from the repository root with Rust 1.94 available and the existing source
Python dependencies (`numpy`, `pycapnp`, `pyzmq`, native `msgq`) available. Build
and reuse the same Cargo target directory; no Python module is invoked by the
production binary.

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-hardwared --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml -p openpilot-hardwared --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-hardwared --bins --examples --locked
PYTHONPATH=. python rust/tools/check_hardwared.py "$CARGO_TARGET_DIR/debug/examples/hardwared_trace" "$EVIDENCE/source"
PYTHONPATH="$MSGQ_PYTHON:." python rust/tools/check_hardwared_daemon.py --binary "$CARGO_TARGET_DIR/debug/openpilot-hardwared" --output "$EVIDENCE/ipc"
```

`check_hardwared.py` compares 12 named source/native cases: three fan profiles,
normal/cycle transitions, exact five-second disconnect expiry, startup blocking/boot latch, thermal
hysteresis, offroad danger threshold, Tesla keep-awake, between-tick ignition,
power integration/shutdown boundaries and low voltage. Float calculations use
absolute 1e-8 plus relative 1e-12 tolerance; branch outcomes and integer/fan values
must match exactly. JSONL native results and original-source inputs/results are
retained with `result.json`.

`check_hardwared_daemon.py` publishes Panda/peripheral/selfdrive inputs through
real native msgq using original Python peers at the source Panda cadence. It
checks valid deviceState messages and timestamps, the five state transitions,
fan outputs, requested-cycle reset, engagement, persisted status packet, uptime,
NetworkMetered, real logging/stats and SIGTERM exit zero. Invalid input event
validity is intentionally supplied: the source does not gate ignition on that
flag. It captures raw `.capnp` events, daemon output, Params values, records,
metrics and `result.json`. All filesystem fixtures and IPC names are owned and
removed when the scenario exits.

Local evidence is under `.omo/evidence/hardwared-111-*` in the main checkout,
with a receipt naming each exact invocation and artifact. The Actions workspace
checks and generic aarch64 release build must pass at the integration SHA.
Neither host tests nor cross-compilation are device acceptance.
