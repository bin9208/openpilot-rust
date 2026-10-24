# Original tombstoned live checker contracts

`original-contracts.json` records the assertions and source/artifact SHA-256 map
from unchanged original Python runtime/shutdown checkers in CI artifact
12146317130, run 39153248437, verified source tree `0416b71d`.
Its immutable SHA-256 is
`e0c391f3d751fd7e8c26445026ac03a65693ba947ae2290977d50ec8c578beab`.

The artificial cases retain the real five-second scanner cadence, enabled/disabled
filesystem behavior, six normal/three error wire messages, six disk messages,
independent original report text, raw Unix names, four startup failures, and three
held-retrace signal observations. Runtime expected report text must come from the
independent native source Session; this fixture supplies no production answers.
The original source SIGINT observation has an orphan sleeper. Cleanup must not
reproduce that leak merely to match an observation; the original assertion only
requires its signal status. Native descendant, crash, HTTP and deadline assertions
remain strict.

Separate runtime/shutdown missing-module REDs were observed with only E0432.
The first native gate passed all eight permanent tests: five runtime tests in
17.87 seconds and three shutdown tests in 17.00 seconds. Scoped strict Clippy
(lib, three binaries and both test targets) passed in 5.80 seconds. No original
expectations or timeout bounds changed.

A subsequent frozen CLI proof passed both tools with `PATH=/nonexistent`: two
runtime cases, four startup failures and three held-retrace shutdown cases. All
44 executable launch attempts (including failed or unfinished attempts) were
audited with zero Python attempts. Five executing ELF hashes were checked before
and after. The ignored evidence bundle is
`.analysis/scratch/2026-10-19-tombstone-live/native-live-owta9oni/`;
`snapshot.json` binds the tested authored sources and `proof.json` records the
actual CLI runs. These are host checks; no device claim or Python retirement is
made here. Original tools remain mandatory source inputs pending family closure.

`worker-abrupt` is an immutable test-only shell launcher. It acknowledges a real
nested session, waits for the observer to retain a pidfd, and exits unsuccessfully.
The regression checks nested-child cleanup, IPC lifetime, foreign-child survival,
and absence of a stale success receipt. It supplies no candidate policy answers.
