# Native sound daemon (#124)

Issue: [#124](https://github.com/bin9208/openpilot-rust/issues/124).
Source: unchanged `openpilot/selfdrive/ui/soundd.py` at integration base
`fd477542`, plus `common/filter_simple.py`, `selfdrive/car/openpilot_toggle.py`,
`system/micd.py` constants, the existing Params/schema, and shipped WAV assets.
Original licensing and provenance remain in the repository.

`openpilot-soundd` owns asset selection, PCM16 WAV decoding, stereo averaging,
linear resampling, sample generation, alert/countdown/timeout selection, volume
filtering, MAIN long-press state and the continuous 20 Hz loop. It consumes
selfdriveState, soundPressure, carrotMan and carState through existing native
msgq/SubMaster and uses the existing native Params and logging implementations.
No Python runtime or sounddevice process is launched.

The source callback's local sample offset and loop count are deliberately not
updated inside its fill loop. Finite alerts may therefore fill an entire
callback past their nominal end, and callbacks spanning a tail repeat that
same tail. The source oracle covers this behavior; the port does not silently
repair it. Source resampling's last-sample weighting is also retained.

Language selection retains SoundLanguageSetting/auto/LanguageSetting, locale
normalization, language-specific assets, English fallback and final prompt
fallback. Reload is checked once per second and resets the sample cursor;
engage/disengage/reverse volume is captured at startup and reused on reload.
Tizi retains its separate engage/disengage files, 30 dB base and volume base 10.
Other devices retain 24 dB and volume base 20. Runtime volume updates only when
no alert is playing; SoundVolumeAdjust is read after the ratekeeper wait.

The source two-second disengaged MAIN trigger and five-to-fifteen-second
selfdrive timeout window are unchanged. Updated selfdrive/carrot messages take
precedence over timeout handling, and validity is not newly used to suppress
alerts. Unsupported alert IDs remain diagnostic events and become none under
the original finish-once rule.

## Native boundary

PortAudio v19 remains an explicit external native dependency, loaded at runtime
as `libportaudio.so.2`. Its host audio driver/backend remains external as well.
`rust/crates/portaudio` owns library/stream/callback lifetimes and exposes an
output stream (float32 mono, 48 kHz, 4096 frames) and a shared input stream for
micd (float32 mono, 16 kHz, 800 frames). Both retain default-device high latency
and flags zero. Rust sample generation does not move into C.

The library initializes once, then terminates/reinitializes before every open
attempt. Soundd retries ten times with the original three-second delay after
each failed attempt, including the last. Start failures are outside that retry
loop. Inactive streams fail rather than falling back to another implementation.
SIGINT/SIGTERM stops the loop/retry wait; stream cleanup precedes callback-memory
release. Buffer ownership and panic containment are tested under both Miri
aliasing models. Actual external calls use an owned compiled ABI fixture.

`--assets` and `--portaudio-library` select explicit fixture paths;
`--cycles` bounds an otherwise continuous process. Normal defaults use the
checkout assets and actual PortAudio. Validation always supplies an owned
library and isolated Params/msgq paths and never initializes host audio.

## Integration and acceptance

The manager catalog advertises an isolated candidate while preserving the
existing production descriptor/predicate. GNU/musl cross-build and Actions
checks remain parent integration gates. Input-specific actual ABI/IPC testing
belongs to the micd work; soundd validates output. Physical speaker output,
AGNOS/device acceptance, complete normal startup/upload, CPU savings and
vehicle behavior remain unvalidated. This component does not satisfy the
whole-runtime device handoff gate in #1.

[Focused validation commands and evidence](../rust-port/soundd-validation.md).
