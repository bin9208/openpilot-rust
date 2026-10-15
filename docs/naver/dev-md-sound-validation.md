# dev-md monitoring warning audio

Final source soundd.py at e8e152dbcfd9574a1791ed12026d1880e87689b3 is retained as
a differential reference. Rust owns PCM generation and runtime event decoding.

The actual event identity now travels with alertSound into native playback.
Stage-two distracted/unresponsive permanent alerts enforce final PCM gain at
least 0.7 for promptDistracted; stage-three warnings use gain 1.0 only for the
matching warningImmediate events. Other events using the same WAV retain their
normal gain, and switching event identity on the same sound clears the DM rule.
A pending finite sound retains its original type until it can stop, matching
source callback semantics. No physical audio device was initialized.

Before implementation, exact-source comparison failed on all seven samples of
the first DM stage-two callback. After implementation, four playback regressions
pass; six actual-source scenarios of 153 steps each compare 6,672 PCM samples
bit-for-bit, plus playback state/event identity. The Params binding's 18 source
hashes were verified. Host all-target clippy passes. ARM build/check result is
recorded after completion; these host checks do not prove device volume.

Source cases include same-wave navigation handoff, both DM warning identities,
unsupported alerts, negative adjustment, language changes, and timeout sounds.
Evidence: `.analysis/scratch/2026-10-15-dev-md-resume/sound-*`.

Fresh ARM sound_trace linked and executed underQEMU with the new source Params
binding:6scenarios,153steps each,6,672PCM samples bit-exact, all playback state
checks pass. No audio device initialized. Evidence sound-arm-v2/summary.json.
