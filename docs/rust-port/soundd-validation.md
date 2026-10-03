# Sound daemon validation

Implementation scope: [soundd #124](../naver/rust-soundd-124.md).
All tests below use owned host fixtures and never open host/device audio.
Parent integration owns Actions and GNU/musl/aarch64 builds.

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --bins --examples --locked
PYTHONPATH=. python rust/tools/check_soundd.py --binary "$TARGET/debug/examples/sound_trace" --binding "$PARAMS_BINDING" --output "$EVIDENCE/source"
PYTHONPATH="$MSGQ_BINDING_ROOT:." python rust/tools/check_soundd_daemon.py --binary "$TARGET/debug/openpilot-soundd" --output "$EVIDENCE/daemon"
```

The existing unchanged Cython Params binding must have its adjacent
`provenance.json`. The source check verifies its binary hash and all 18 source
file hashes directly before use. Python dependencies are numpy, pycapnp and
pyzmq; the daemon check additionally requires the original msgq Python binding.
Python is only the oracle/fixture runner. The source runner executes unchanged
soundd/filter/MAIN definitions and actual original Params; it does not import
sounddevice or initialize audio.

Six source scenarios cover mono/stereo PCM16 at 48 kHz, resampling at 12 kHz
and 44.1 kHz, both device volume policies, synthetic fallback assets and the
actual shipped assets. Each applies 141 state/callback steps: all supported
alerts, unsupported enum/raw IDs, finite/infinite tail behavior, none/finish-once,
countdown changes/repetition, volume filtering/gating, stale-message boundary
conditions, MAIN hold/release and language replacement. Float32 samples compare
bit for bit, including signed-zero output with negative volume adjustment; scalar volume/filter values use 1e-12 absolute/relative tolerance.
Raw input/source/native JSON and summary counts are retained.

Seven actual production-binary scenarios use a compiled PortAudio C ABI fixture
and original isolated msgq. They validate the opened device/channel/format/rate/
blocksize/latency/flags, callbacks and captured float32 output; actual alert,
volume, language, MAIN hold and selfdrive-timeout packets; one continuous PID
with approximately 50 ms main-loop intervals; retry recovery/exhaustion;
start failure; inactive-stream failure; and signal stop during retry and live
playback. Captures include every input Cap'n Proto packet, full callback samples,
ABI lifecycle timestamps, stdout/stderr and exit status. `--mode live` permits
rerunning only the expanded live scenario after fixture changes. Ten failed
opens require about 30 seconds because the real retry delay is preserved.

The shared PortAudio lifecycle probe verifies that start/active/device reject
an unopened stream, the selected device index is retained, and an opened stream
can start/poll/stop/close. Always provide the owned fixture library and capture
paths, never the host library:

```sh
SOUND_FIXTURE_EVENTS="$EVIDENCE/lifecycle/events.jsonl" \
SOUND_FIXTURE_SAMPLES="$EVIDENCE/lifecycle/samples.f32" \
"$TARGET/debug/examples/lifecycle" "$EVIDENCE/daemon/libowned-portaudio.so"
```

The native C ABI cannot execute under Miri. Its production calls are gated by
`native-skip-miri`; the exact Rust callback boundary remains testable without
that feature. The three callback tests cover output initialization, input
borrowing and panic containment. Both models are required:

```sh
MIRIFLAGS='-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check -Zmiri-preemption-rate=0.5' cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml -p openpilot-portaudio --no-default-features
MIRIFLAGS='-Zmiri-tree-borrows -Zmiri-strict-provenance -Zmiri-symbolic-alignment-check -Zmiri-preemption-rate=0.5' cargo +nightly-2026-09-29 miri test --manifest-path rust/Cargo.toml -p openpilot-portaudio --no-default-features
```

Local raw captures and the exact-invocation receipt are under
`.omo/evidence/soundd-124/` in the main checkout. Full startup/upload integration,
physical audio output, AGNOS/device acceptance and performance remain separate.
