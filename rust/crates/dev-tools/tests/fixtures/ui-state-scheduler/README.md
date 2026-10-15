Full fixed-input original UIState/Device and DisplayScheduler gate captures.
State: big/mici/pc, 800 frames each. Scheduler: disabled/enabled, 80 steps each.
Expected outputs were produced by original Python class bodies, never by the
candidate Rust implementations. Serialized CarParamsPersistent bytes and schema
SHA256 are recorded in provenance.json. Corpus and compressed SHA256/lengths are
in integrity.json. Python capture drivers remain scratch-only, not runtime tools.

Candidate comparison retains Python JSON numeric equality, the original State
flattening precedence, absolute float bound 1e-11, and ordered scheduler calls.
A fixed expected source value does not replace arbitrary-input state_source.py
or the original runtime's functionality; those remain pending. The original
source-interpreter failure cases in failures.json are provenance only. Native
fixed-oracle tests exclude source-* cases: no Python source subprocess remains,
and no test-only subprocess stands in for that removed dependency. Source and
corpus-integrity pins fail closed before any candidate invocation/output mutation.
For admitted pins, captured original source artifacts are written at their
original phase, including State's '(no stderr)\n' source log fallback.

Captured proof is Linux x86_64 / CPython 3.12.15. It does not establish original
ARM numeric equivalence and introduces no new host rejection. Relevant source or
locked dependency changes require independent original recapture; the native
checker does not silently keep stale expected output or derive it from candidate
code. The original CLI exposes only --binary and --output; arbitrary/random input
capabilities of the retained source helper are outside this fixed-gate migration.

Actual current-source state_trace/scheduler_trace were compared through both
unchanged originals and native checkers: all25 files and stdout match byte for
byte. actual-failures.json adds12 real-subprocess failure/newline comparisons.
raw-comparison.json proves admitted raw candidate/output paths, inherited raw
environment values and State's PYTHONPATH replacement. An original State checker
copied beneath a physical raw-byte root fails pycapnp's Unicode schema loader
before scene creation; raw-root-original.json records that removed interpreter
limitation. The native CLI's raw-root test establishes lossless native capability,
not original successful raw-root equivalence. No new raw-root rejection is added.
