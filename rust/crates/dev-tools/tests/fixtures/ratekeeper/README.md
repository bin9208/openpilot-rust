# Independent shared Ratekeeper validation corpus

This bounded checker retires `check_selfdrived_ratekeeper.py`, the existing
20,000-step differential for the Ratekeeper/MovingAverage classes also used by
beep_source.py. It does not retire the broader beep policy or live daemon tools.
Its sole production caller is the selfdrived source-comparison workflow.

The unmodified original main generated every fixed-seed input and evaluated the
original AST-extracted Ratekeeper and MovingAverage classes before capture at
the subprocess boundary. No native candidate supplied an expected result.
All 20,000 complete timing states, booleans, scalar sums and 100-slot moving
average buffers are preserved as original JSON lines, compressed with input
and pinned through source/interpreter provenance, lengths and both SHA256s.
The original generates 49,637,932 uncompressed JSON bytes; the compressed fixture
is approximately 1.5 MiB. These are synthetic clock inputs, not device captures.

Sixteen original-main captures preserve output files by exact size/SHA256,
stdout, comparison status and process call count. They include malformed JSON,
cardinality, NaN-free numeric/bool equality, UTF-8/BOM/NUL, universal and Unicode
line separators, diagnostic stderr, subprocess failure, and binary disappearance
after a successful comparison. Six original CLI captures retain parser statuses.
Three additional original artifact-write failures preserve the order of
native.jsonl, native.stderr and the success manifest. All seven native integration/routing tests pass after retirement, including
missing live-source and changed reintroduced-checker guards.

The native gate writes input/source files before candidate execution, preserves
exist_ok=False, writes native stdout/stderr before JSON parsing/comparison,
compares arbitrary JSON with Python equality, hashes the supplied binary path
after equality, then writes/prints the exact original manifest. It validates
live runtime source hashes; only the fixed checker may be absent after retirement.
No interpreter, generic Python wrapper, new native dependency or real sleep is
needed by production. The original classes remain required by other tool families.

The actual Rust ratekeeper example passes all 20,000 states. All five output
files and stdout match the original checker byte-for-byte. After deleting the
Python checker, PATH=/nonexistent produces the same files and stdout. Strict
Clippy passes. Heavy corpus coverage runs in its required CI lane, preserving
the fast aggregate gate. These are host checks, not device validation.
