Complete original fixed-input checker corpora, captured using Python 3.12.15,
qrcode 8.2 and numpy 2.5.3 on Linux x86_64. `provenance.json` records source,
interpreter, dependency and platform hashes. `corpus.json` records complete input
and original expected-output sizes and SHA256; gzip members have mtime zero.
Expected output was calculated by the unchanged original Python sources, never
by the native UI implementation. `failures.json` captures actual original child
process behavior and output-file hashes; replies are content-addressed gzip data.

These assets replace only two fixed-input CLI comparison gates. Runtime QR/torque
implementations and other original consumers remain independent work. No ARM
original-output equivalence is established here. A relevant locked dependency or
original torque source change requires independent recapture; unlike the original
checker, the native gate deliberately fails closed rather than recomputing stale
expectations. No random/custom corpus input interface is removed: the original
commands expose only --binary and --output.
