# Independent original camera misc checker captures

Expected values were captured from the unchanged Python checker before writing
the native replacement. CPython version and original checker SHA256 are recorded
in every fixture. The shared original equal/f32 helper remains a live Python
dependency of the state checker and is not retired by this family.

The real current-source C++ run passed all 7,292 cases: timing 5,007,
NV12 1,644, DMI 180, contiguous writes 139, random writes 139 and luminance
183. Every fixture byte count and SHA256 is recorded, including all seeded random
pixel images. Inputs include absolute fixture paths; both exact and normalized
output-path hashes are captured. Production must derive requests and pixel data
from the original deterministic algorithm, never consume a fixture input blob.

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
