# Native microphone validation

Issue [#126](https://github.com/bin9208/openpilot-rust/issues/126) ports the
project-owned runtime in `openpilot/system/micd.py`. It uses external PortAudio
v19 through the shared #124 owner, with mono float32 16kHz input, 800-frame
callbacks, 1600-sample analysis and 10Hz `soundPressure` publication. RustFFT
6.4.1 supplies the FFT implementation; its planner and scratch buffers are
created once. The Hann window and normalized A-weighting follow the original
formulas. No audio input, thresholds or sound policy is recalibrated.

`rawAudioData` preserves native int16 bytes after float32 multiplication by
32767, including wrapping and invalid conversion behavior in the host oracle.
The callback appends float32 samples as float64, processes complete windows,
and retains the final pressure plus partial window. Raw publication happens
synchronously on the callback thread before calculation. That thread owns its
msgq publisher in TLS; no unsafe Send or audio queue is introduced. Pressure
publication uses a separate main-thread publisher and a mutex snapshot. On
callback error, input stops while main continues the last pressure, as in the
source sounddevice callback boundary. TLS is dropped on callback-observed
shutdown/error, otherwise when the native worker or process exits.

The source comparison executes unchanged numerical functions and callback
body against 38,490 samples in 35 irregular callbacks: silence, DC, tones,
noise, tiny amplitudes, overflow/nonfinite casts, and recovery windows. Before
comparison, float64 tolerances were set to relative 1e-10 / absolute 1e-12;
raw bytes, pending count and nonfinite classifications must match exactly.
Observed maximum absolute errors were 3.50e-15 unweighted, 3.61e-16 weighted,
and 1.43e-14 dB. These numerical tolerances do not change runtime thresholds.

The actual executable is exercised through an owned PortAudio v19 library that
checks ABI parameters and invokes the real callback on a pthread. Original
msgq/cereal peers verify all sixteen 800-sample raw packets, source pressure
values and 10Hz publication. Four scenarios cover normal capture, failed-open
retry after three seconds, SIGTERM during retry, and failed stream start.
Stop/close/terminate order is checked. Source does not poll stream activity;
the fixture aborts if the native microphone starts doing so. The three shared
PortAudio callback ownership tests passed strict Stacked/Tree Borrows Miri in
#124; no claim is made that Miri executes a native PortAudio library.

```sh
cargo build --manifest-path rust/Cargo.toml -p openpilot-micd --bins --examples --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-micd --all-targets --locked -- -D warnings
python rust/tools/check_micd_analysis.py --binary "$CARGO_TARGET_DIR/debug/examples/micd_trace" --output "$EVIDENCE/analysis"
PYTHONPATH="$MSGQ_BINDING:$PWD" python rust/tools/check_micd_daemon.py --binary "$CARGO_TARGET_DIR/debug/openpilot-micd" --output "$EVIDENCE/daemon"
```

The source oracle needs numpy; the daemon gate additionally needs the original
msgq Python binding, pycapnp, and a C compiler with pthread support. Captures
include source SHA, numerical/raw comparisons, actual cereal packets, native
output and timestamped ABI events. Parent integration owns required Actions
host/ARM checks. No desktop microphone, device, vehicle, physical audio latency,
CPU saving or complete-runtime acceptance was tested. `libportaudio.so.2` must
be supplied by the target OS; the generic musl build alone does not establish
compatibility with the AGNOS GNU shared library.
