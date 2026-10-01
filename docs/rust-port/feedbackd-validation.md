# Native feedback daemon

Issue [#131](https://github.com/bin9208/openpilot-rust/issues/131), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1).

`openpilot-feedbackd` ports the active loop in `openpilot/selfdrive/ui/feedback/feedbackd.py`. It opens Params, owns the original two publication endpoints and three subscriptions, logs each bookmark input update and publishes a valid empty `userBookmark` with the current monotonic timestamp. An input's validity flag does not suppress the bookmark, matching the source.

The original `if False` branch disables LKAS-triggered audio recording. The Rust runtime preserves that disabled state even with `RecordAudioFeedback` enabled. It does not introduce a recording setting or enable dormant behavior. SIGINT/SIGTERM stop the native loop and release IPC ownership.

## Evidence

`rust/tools/check_feedbackd.py` executes the unchanged source main body with real original messaging and the unchanged compiled Params binding. Only imports, paths and microphone constants are supplied by the host fixture. The same sequence then drives the actual Rust executable through original msgq/cereal peers: eight iterations, four bookmark events, valid and invalid inputs, LKAS press/release and raw audio traffic. Both sides produce the same four bookmarks, no audio packets, four source-compatible log records and a clean SIGINT exit. Raw packets and source hash are retained.

Run with an original msgq Python binding on `PYTHONPATH`:

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-feedbackd --locked
python rust/tools/check_feedbackd.py --binary rust/target/debug/openpilot-feedbackd --binding PARAMS_BINDING.so --output EVIDENCE
```

The initial host fixture incorrectly opened output subscribers before native publishers and tripped the existing queue-size guard. It now waits for actual input-reader readiness before subscribing to the created output queues. Source/native IPC passes; focused clippy, fmt and diff checks pass. CI and aarch64 checks remain integration requirements. No physical audio/device operation or production selection occurred. Original msgq/CXX and Linux clocks/signals remain native dependencies; complete startup/upload is still pending #1.
