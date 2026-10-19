# Unchanged continuous statistics source observations

Pre-implementation captures from the actual unchanged Python controllers with
real original Params/msgq/log collector and the fresh native daemon examples.
All ten original/native scenarios passed: continuous operation, eight failure
boundaries and the log-failure fallback. The source-only results are retained in
`original.json`; expected results are not derived from native output.

`unicode-digits.json` is the full CPython 3.12 / Unicode 15.0 Nd input enumeration
(680 code points), preserving the original controller's complete input sequence.
`manifest.json` binds both fixtures and21 relevant source/schema/helper inputs.
Private capture commands and raw results: `.analysis/scratch/2026-10-19-stats-daemon-tools/`.

The native controllers replay these ten fixed scenarios against real processes,
Params files, msgq/ZMQ and the existing independent collector. They compare exact
statistics contents/modes, state/flush boundaries, failure outcomes and collector
records with this capture. They do not implement a general Python statsd oracle.
An actual DEBUG publication acknowledges collector readiness before the candidate
starts; it is drained before the scenario and produces no disk record. Endpoint
ownership is acknowledged separately with a held filesystem inode, so cancelling
the controller preserves a replacement socket owned by another process.

The three controllers have passed same-ELF before/after removal checks (ten cases
and34 execs each, no Python with an empty PATH). The later interrupted-receive
closure also retired `stats_source.py` and `check_stats_interrupted.py`; see
`../stats-interrupted/README.md`. Original `statsd.py` remains. Fresh post-removal
rebuild and committed-head CI are separate gates, recorded in the #296 evidence.
