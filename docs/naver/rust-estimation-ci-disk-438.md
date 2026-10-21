# Estimation CI disk admission (#438)

Actual PR source `8e8b1e8f63964302541a0323b6af276d1a876c7d` failed in
[run39524132398/job118974707859](https://github.com/bin9208/openpilot-rust/actions/runs/39524132398/job/118974707859).
The native loop CLI completed, but four unrelated FFmpeg builder unit tests
stopped at disk admission before the loop integration/reference tests. Three
report insufficient25GiB+growth capacity; one expected fake-runner commands
that admission correctly prevented. No disk floor or functional assertion is
being lowered.

The failed log records89,630,654,464bytes near initial work and29,476,712,448bytes
before the default library suite. Several feature-specific Cargo compilations
are followed by13–14GB drops. The estimator job omitted dev/test debug settings,
while its later fixture step and other deep jobs use zero debug info. Job-level
`CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0` now prevent those
large debug outputs and keep a consistent profile for the whole job. Two build
jobs and disabled incremental compilation remain. Source policy requires both
settings.

The corrected complete estimator job passed at checked-out source
`20a8f52e83d85c7c5113c5484179b0c016a034eb` in
[run39529411343/job118991609255](https://github.com/bin9208/openpilot-rust/actions/runs/39529411343/job/118991609255).
The one-time workflow push was `512d1f6b9e54046e7e15587cd314057de0893600`,
separate from that checked-out source. The log records 44 disk admissions;
the lowest free space at an admission was81,706,479,616bytes and the smallest
free-minus-required margin was47,911,337,984bytes. Incremental compilation and
both dev/test debug profiles were zero throughout. The job completed the broad
locationd, paramsd, lagd, Eigen, daemon/CLI and final library suites. The
artifact records 39 paramsd loop cases/2,420 frames, 1,202+8 native paramsd
publications, lagd's 95 numeric cases and 1,201/1,301/1,201-frame loop modes.
The final masked-allocation record compares the old and current implementations
with equal fingerprints: at lengths64 and128 over16 iterations, each saves32
allocation calls and respectively16,384 and32,768 requested bytes. This is a
host probe result, not a vehicle CPU or memory measurement. Artifact:
[`rust-estimation-evidence` #12315193687](https://github.com/bin9208/openpilot-rust/actions/runs/39529411343/artifacts/12315193687),
ZIP SHA-256 `10c74ad5b95e526eff67b3fa886b174a0273125b33bac9d3cde4726c7e5aeab3`.

This is a CI disk fix. The observed capacity changes do not establish vehicle
memory savings or performance. Local validation was syntax/format/diff only.
The companion workspace format failure was module declaration order, repaired
in3a57116f; this focused estimator success does not by itself establish the
separate full workspace gate.
