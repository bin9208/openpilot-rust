# Original camera packet checker evidence

All expectations were captured from unchanged check_camerad_packets.py before
native implementation. Metadata pins the checker SHA256 and CPython version.

The real current-source C++ reference versus current Rust sensor_packets example
passed all 3,281 cases and compared 5,869,496 packet bytes. Generation captures
every case hash, the complete inputs.jsonl hash, and source/native request hashes
for all 66 batches. Production must regenerate the fixed algorithm with seed 187
and live native/C++ subprocesses; no canned production input blob is permitted.

The 35 process cases independently capture exact argv/requests, stdout, artifacts
and overwrite/failure phases. Both children execute before source/native output
files are written. Each lane writes its stdout and stderr before checking that
lane's exit status; a source exit failure leaves native files absent. A mismatch
writes failure.json without a trailing newline. Byte counts preserve original
iterable and length semantics, including text codepoints and object keys.
The seven CLI cases cover required/unknown options and runner grammar. Ten
original deep-JSON outcomes separately pin parser and receipt-serialization
limits; an actual original CLI run with raw Unix runner/output paths preserves
exact report/stdout and all 266 generated artifact entries.

Capture drivers and original full proof output live under ignored
.analysis/scratch/2026-10-18-camera-packet-check/. The intended missing-module/bin RED was observed before production code.
All five native tests and strict Clippy pass. After removing the Python CLI,
the PATH-disabled current-source comparison passes all 3,281 cases/5,869,496
bytes with byte-identical original report and inputs. Caller migration is native.
No device result is claimed.
