# Dashcam worker environment decoding (#249)

The native upload worker collected every environment variable as a UTF-8 string
while constructing its runtime settings. An unrelated value containing byte
`0xff` caused a panic before the worker emitted a protocol packet or contacted
the upload recipient. The original upload completed successfully with the same
inherited environment and owned input files.

The worker now reads only its seven consumed serial, webhook and concurrency
keys. Missing values retain their existing defaults; relevant named string
values retain the native UTF-8 contract. The existing URL/token provider and
upload engine, manager, metadata, report and transport implementations are
unchanged.

The retained failing worker has SHA256
`2e3ff170ea1cf71760da3c73b2c7f2b3eed969c6c9f0733e6aec80a854c4286b`.
The corrected worker has SHA256
`788ed97be48d2b2566dd5fdbb88debdb1a5fc6d9a4cf460f9766a7d26d594d4b`.
Two actual source/native comparisons match: the unrelated non-UTF-8 case and
the normal environment case. Each sends four matching owned HTTP requests;
the native worker exits zero with a successful terminal result. The ordinary
normalized output also matches the retained prior worker.

Strict all-target Clippy, formatting and diff checks pass. The portable
regression entrypoint is `rust/tools/carrot_server_dashcam_sync_probe.py` with
`--worker PATH --worker-controls --expect-env-fixed --output NEW_DIR` and the
existing source dependencies supplied by the caller. It asserts terminal result,
response equality and captured upload bytes. A malformed-readiness control
also verifies that setup failure preserves its original exception, reaps its
owned child and returns the parent file-descriptor count to baseline.

The reproduction is under
`.omo/evidence/carrot-server-225-resume/dashcam-sync/probe-final/non-utf8/`;
the corrected comparisons are under
`.omo/evidence/carrot-server-225-resume/dashcam-worker-env-249/green/`.
These use local recipients and owned files, Params and repositories. No vehicle,
NAS or actual Discord recipient is involved. The synchronous HTTP route and
complete server/runtime integration remain separate work under #225 and #1.
