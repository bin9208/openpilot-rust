Independent soundd daemon checker observations
============================================

Only `rust/tools/check_soundd_daemon.py` is a retirement candidate in this unit.
The owned PortAudio C ABI fixture and the explicit native WAV fixture-assets
helper remain. The controller imports neither soundd policy nor runtime sample
constants to obtain expected results. No physical audio device is exercised by
these fixtures: CI must compile/reuse the verified owned C fixture, never the
system PortAudio library.

The immutable manifest binds 21 original inputs: the checker, owned C fixture,
four full cereal schemas, services.py, and fourteen msgq implementation files.
Seven retained event streams came from the unchanged original checker running
all seven modes against the earlier source-sealed native daemon. They are
historical observations, not proof that the new controller or current candidate
passes. Additional source-backed captures cover 18 event/time controls, 21 raw
sample controls, ten full cereal packets, fifteen CLI cases, and six actual
failure/artifact stages. Capture-time numpy/pycapnp/pyzmq identities and the
original source hashes are in manifest.json. No original Python implementation
is retained under a new extension.

The source assertion captures preserve strict cadence/retry inequalities and
float32 equality. MAIN checks every column of the first twenty buffers; timeout
checks every column of buffers 2..55, with the warning in the first column at
buffer 61 or later. Negative zero compares equal to zero; unused NaN samples
are not globally rejected. NumPy ignores up to three trailing partial-float
bytes before reshaping; an extra whole float fails reshape. Packet expectations
are unchanged original full-schema bytes at fixed captured timestamps. Runtime
inputs use the real monotonic clock and real native publishers, not these bytes.

Native tests require current daemon/checker/assets-helper/owned-ABI artifacts
through SOUNDD_DAEMON_BINARY, SOUNDD_DAEMON_WORKER_BINARY,
SOUNDD_FIXTURE_ASSETS_BINARY and SOUNDD_PORTAUDIO_LIBRARY. Missing prerequisites
fail instead of silently returning. SOUNDD_DAEMON_TEST_OUTPUT retains raw events,
samples, packets, logs, summaries and ownership witnesses for CI diagnostics.
The seven real modes remain mandatory, alongside exact CLI/failure controls,
the default owned-C compilation path, broken stdout, and actual cancellation.

Additional source-only regressions require caller stdin to reach an immutable
observing launcher through both workers before the real native WAV helper runs.
A full stdout pipe acknowledges the completed bounded scenario and final root
publication boundary; the test then mutates only a private copied source or
daemon artifact, drains stdout, and requires failure without a new PASS receipt.
Receipt cleanup retains the persisted file descriptor, including after unlink,
so a foreign replacement cannot recycle the owned inode before cleanup. Signal
handlers remain installed through that final cleanup check. These new tests are
pending CI execution; unchanged original captures above have not been refreshed.

The replacement adds explicit compiler (60s), fixture-assets (30s), and private
worker (90s) ownership bounds; the original compiler/assets calls were unbounded.
Original per-operation 5/3/4/7/10/35s waits, 2s shutdown and sub-second stop-retry
assertions remain intact. Those new safety caps are intentional differences,
not a claim of equivalence to an unbounded subprocess. Before/after source and
regular-file executable identities detect bounded replacement, not arbitrary
ABA mutation. Only genuine absence of this exact retired checker is permitted;
all other original inputs remain mandatory, including after deletion. Actual
live hashes and the historical checker hash are reported separately.

Status: source and tests prepared under the user's cloud-validation workflow.
Only local formatting/syntax checks are permitted. No current native build,
new local test pass, or completed retirement is claimed. GitHub Actions must run
all tests/current seven-mode proof and retain diagnostics before deletion.
