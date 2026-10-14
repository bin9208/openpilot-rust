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
publication uses a separate main-thread publisher and a mutex snapshot. The #300
lifecycle correction checks stream activity before each pressure publication.
Callback abort, inactive input and activity-query errors terminate
the daemon for supervisor restart instead of publishing stale valid pressure.
This intentionally fixes the inherited sounddevice callback boundary; successful
capture analysis, raw bytes, cadence and message validity are unchanged. TLS is
dropped on callback-observed shutdown/error, otherwise when the native worker or
process exits.

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
values and 10Hz publication. Seven scenarios cover normal capture, failed-open
retry after three seconds, SIGTERM during retry, failed stream start, inactive
capture, callback abort and activity-query failure.
Stop/close/terminate order is checked. Source does not poll stream activity; #300
explicitly corrects that lifecycle behavior. The owned fixture models active,
inactive and failed activity queries,
and the correction cases require failure before another pressure publication.
The unchanged source waveform oracle remains required. Native Rust process
tests also cover the three capture failures without Python bindings. The three
shared PortAudio callback ownership tests passed strict Stacked/Tree Borrows Miri in
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

On 2026-10-14, #300 native lifecycle tests first reproduced all three failure
cases: the unchanged daemon stayed alive beyond the three-second bound after
capture completed or aborted, and never queried stream activity. The normal
finite case exited successfully but also lacked activity queries. After the
guard was added, `cargo test --locked -p openpilot-micd` passed all four
lifecycle tests, including ordered stop/close/terminate and successful finite
capture. `cargo clippy --locked -p openpilot-micd --all-targets -- -D warnings`
passed. Both commands used the coordinated target cache, two jobs and disabled
incremental compilation, with 389 GiB available before each build. The original
Python daemon oracle could not be rerun locally because `msgq.ipc_pyx` is absent;
its waveform/cadence and added lifecycle checks remain a separate required gate.

Later on 2026-10-14, that original daemon gate was rerun using the private
original msgq binding and current native debug executable. All seven cases
passed: `live`, `retry`, `stop-retry`, `start-failed`, `inactive`,
`callback-abort` and `activity-query-failed`. The live case retained 16 raw
packets, 15 pressure packets and a 0.1000298655-second mean pressure interval;
the original waveform and cadence assertions were preserved. The pass log is
`.analysis/scratch/2026-10-14-agnos296/micd-daemon.log`; case packets and IPC
records are under `.analysis/scratch/2026-10-14-micd300-daemon/` in the active
worktree. This resolves the earlier missing-binding host validation blocker.
It does not establish physical microphone or device restart behavior.

The separate original waveform analysis oracle also passed: source SHA256
`cfafc90503405f481f666618871a6a25f6305a8ca38cf87bcafc154c3a375164`,
35 callbacks and 38,490 samples, exact raw bytes and maximum absolute numeric
differences `[3.497202527569243e-15, 3.608224830031759e-16, 1.4210854715202004e-14]`.
Its record is `.analysis/scratch/2026-10-14-micd300-analysis/comparison.json`.
The retry daemon case also retained 16 raw/15 pressure packets, with mean
pressure interval 0.09999795528571429 seconds. Python test orchestration remains
in the retirement inventory; passing these oracles does not retire that source.
