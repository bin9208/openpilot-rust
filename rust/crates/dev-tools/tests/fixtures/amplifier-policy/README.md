# Independent MAX98089 policy corpus

Captured on 2026-10-18 by running every `cases()` and `source()` result from
`rust/tools/check_amplifier.py` against the retained original amplifier source.
The native amplifier was not used to construct expected values. `manifest.json`
records both source hashes, compressed and expanded hashes, size and case count.

All 2,034 original cases are retained: all 256 initial register bytes for each
operation, ordered transactions, debug output, failures at every event, persistent
failure kinds, retry and close precedence, unknown models and four seeded register
arrays. This is the original fixed regression input set, not all-input or hardware
validation. Changing the source policy requires independent recapture and review;
the native checker rejects source drift while that implementation remains present.

The generic amplifier runtime and native Linux I2C integration have separate
coverage. This corpus does not replace those modules or tests.

The fixed generator is now retired with its reviewed SHA256 explicitly embedded
in the native checker. Missing retired generator is allowed; a changed generator
reintroduced at its old path is rejected. The live generic amplifier source must
still match its pin. Nested duplicate keys are checked before overwritten values
can disappear; decoded UTF-8 text is never reinterpreted as UTF-16/32 bytes.
