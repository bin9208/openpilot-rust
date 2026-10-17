# Independent original camera exposure checker captures

Expected values were captured from the unchanged Python checker before writing
the native replacement. CPython version and original checker SHA256 are recorded
in every fixture. The shared original equal/f32 helper remains a live Python
dependency of the state checker and is not retired by this family.

The real current-source C++ run passed all 54,207 observations: 99 scenarios
and 54,108 steps at the original maximum/default 1,200 frames. Source input SHA256
is 3afb445024ea99d76695c8ae4cb77ff6e684e38d6d33838fecb424c3a187da09;
native input SHA256 is
bbd6302e503c3f4550db59a449b496e91cc585735283b0f1f88fa4309d8225ac.
The 16 frame-admission captures include Unicode decimal digits, underscores,
sign/whitespace, frame bounds, and the 4,300-digit integer limit.

Process captures preserve child argv, canonical request hashes, exact receipts,
stdout and preexisting-output behavior across comparison, invalid JSON,
cardinality, child exit, Unicode decoding, runner grammar, hash-read and artifact
write failures. Deep original CLI probes distinguish successful recursion,
comparison failure and failure-receipt serialization error.

Capture scripts and actual-source proof outputs are in ignored
.analysis/scratch/2026-10-18-camera-pair-check/. The intended missing-module/bin RED was observed before implementation. Eleven native tests and strict scoped Clippy pass. Both actual post-removal
native comparisons pass with PATH disabled and byte-identical original reports.
The misc Python CLI is removed; exposure retains only the unchanged quoted helper
for the pending state checker. No device/hardware result is claimed.
