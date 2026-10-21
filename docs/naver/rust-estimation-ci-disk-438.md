# Estimation CI disk admission (#438)

Actual PR source `8e8b1e8f63964302541a0323b6af276d1a876c7d` failed in
[run39524132398/job118974707859](https://github.com/bin9208/openpilot-rust/actions/runs/39524132398/job/118974707859).
The native loop CLI completed, but four unrelated FFmpeg builder unit tests
stopped at disk admission before the loop integration/reference tests. Three
report insufficient25GiB+growth capacity; one expected fake-runner commands
that admission correctly prevented. No disk floor or functional assertion is
being lowered.

The log records89,630,654,464bytes near initial work and29,476,712,448bytes
before the default library suite. Several feature-specific Cargo compilations
are followed by13–14GB drops. The estimator job omitted dev/test debug settings,
while its later fixture step and other deep jobs use zero debug info. Job-level
`CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0` now prevent those
large debug outputs and keep a consistent profile for the whole job. Two build
jobs and disabled incremental compilation remain. Source policy requires both
settings; the corrected full estimator job still needs fresh cloud execution.

This is a CI disk fix. The observed capacity changes do not establish vehicle
memory savings or performance. Local validation was syntax/format/diff only.
The companion workspace format failure was module declaration order, repaired
in3a57116f; full workspace execution remains required after that repair.
