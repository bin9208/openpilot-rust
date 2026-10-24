# Independent camera constant generator results

`bps-original.json` contains41 unchanged-original BPS executions:33 initial
source/value/failure cases and8 Unicode regression cases. These preserve array
and row counts, C zero filling, comment removal, exact extraction, universal
newlines, raw-source hashing, base-zero integer forms, and partial stdout before
failure. The late byte failure occurs after two complete arrays are printed.
Output provenance comments intentionally remain byte-identical.

The BPS implementation uses captured CPython3.12.15 / Unicode15.0.0 word,
decimal, and uppercase tables. The original and the earlier native binary
independently reproduced the Unicode-version mismatch before correction.
Exhaustive digests cover all1,114,112 Python codepoints, including surrogate
behavior; the native regression checks all three digests. Data provenance and
encodings are in `unicode_capture`; the existing Unicode license is retained in
`data/unicode-15.0.0/LICENSE.txt`. No current Rust Unicode property or special-case
codepoint blocklist determines source admission.

`sensor-original.json` contains70 unchanged-original sensor-generator cases.
They preserve exact stdout bytes and child requests, f32 bit patterns, Python
string/container representation, JSON ordering, strict three-configuration zip
failure phases, child errors, exposure register conversion, Unicode line breaks,
deep values, and Unix path/surrogate-escape behavior. Raw-output cases use
`stdout_hex`. Captures precede their corresponding native implementation changes;
expected answers are never obtained from the replacement.

Observed native gates passed41 BPS cases plus exhaustive Unicode and70 sensor
cases plus actual native CLI child tests. Against a freshly built current-source
C++ sensor reference, original and native stdout matched all45,354 bytes
(SHA256 `aae04c695b5964a2cf92e453faff745117e6306a33b750004dfa42d8bdc53a00`).
With `PATH=/nonexistent`, the trace contains the native generator and its three
actual C++ reference calls, without Python. Current generated sensor/BPS data
also matched the tracked tables after Rust formatting.

Only the sensor generator requires `camera-sensor-native`; its shared runtime
formatter dependency is optional. This keeps Cap'n Proto/ZMQ dependencies out of
default fast tool builds while the camera job enables the complete native gate.
Original sensor C++ sources remain live inputs and comparison checkers remain
independent. These are host/source results, not hardware or device acceptance.

The parent confirmed the post-deletion sensor gate passed all four tests and
strict Clippy. `sensor-paths-original.json` adds10 independently captured path
lookup cases (80 sensor cases total), covering `./`, redundant separators,
retained `..`, raw Unix bytes, literal `~`, and positive/negative PATH lookup.
The repaired native subprocess argv preserves Python Path lexical normalization. Local captures, generated C++, process traces, and receipts
remain under ignored `.analysis/scratch/2026-10-18-camera-data/` and
`.analysis/scratch/2026-10-18-camera-lifecycle-generators/`.
