Independent soundd daemon checker observations
============================================

`rust/tools/check_soundd_daemon.py` was retired after the accepted f19
same-ELF cloud proof. Its original source hash and observations remain sealed
in the fixtures; its implementation is not retained under another extension.
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
passed the f19 cloud run; unchanged original captures above have not been refreshed.

The replacement adds explicit compiler (60s), fixture-assets (30s), and private
worker (90s) ownership bounds; the original compiler/assets calls were unbounded.
Original per-operation 5/3/4/7/10/35s waits, 2s shutdown and sub-second stop-retry
assertions remain intact. Those new safety caps are intentional differences,
not a claim of equivalence to an unbounded subprocess. Before/after source and
regular-file executable identities detect bounded replacement, not arbitrary
ABA mutation. Only genuine absence of this exact retired checker is permitted;
reappearance fails source admission and final verification.
all other original inputs remain mandatory, including after deletion. Actual
live hashes and the historical checker hash are reported separately.

Status: run 39512282992, job 118935976457, passed the old-median behavioral
RED control, all seven present and absent native modes, both exact
22-successful-exec audits, same four executable/library hashes, and strict
Clippy. The accepted evidence ZIP is artifact 12306223105, SHA-256
249dcaa0ff9c5c82a27c8cbc93bbddf83ecc79f8320546b8e4c4a373576293b1.
This source deletion and checkout-absent caller change still require a fresh
GitHub Actions pass; no local build or functional pass is claimed.

Retirement observation prepared from the actual successful sensor/audio job
118914882870 at checked-out source f60e4ac0a655592aaef659f35cc354b86ffa09a0:
`original-seven-f60.json` is the unchanged Python controller's complete root
summary, copied byte-for-byte from artifact 12304571676. Its SHA and the ZIP,
workflow, source and checker identities are in `original-seven-f60.provenance.json`.
This small report contains process IDs and monotonic timestamps but no WAV
capture, credential or native-derived expected value. The reference test seals
the raw report and compares stable mode/order/status/open-count/packet values;
each fresh lane independently checks the variable PID, callbacks, median period,
and actual event trace with the original strict predicates. The accepted f19
job ran the unchanged Python original and both native lanes. After deletion,
the reference compares the sealed original observation to two fresh native
runs: the actual checkout root and a copied absent root.
`SOUNDD_NATIVE_PRESENT_EXEC` and `SOUNDD_NATIVE_ABSENT_EXEC` each require 22
successful execs, reject unparsed/`execveat` attempts, and check one
controller, seven same-ELF private workers, seven asset helpers, and seven
daemons with linked per-mode temporary roots. The fixture alone does not
authenticate an arbitrary remote artifact at runtime.
