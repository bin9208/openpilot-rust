# paramsd startup gate foundation

The five cursor/release cases are translated from the unchanged
`rust/tools/tests/test_paramsd_atomic_gate.py`: normal commit, wrap, 32-bit
cycle overflow, wrap header without the payload, and a different payload
length. The Rust cases use real CLOEXEC pipes and a 256,984-byte queue with
the cursor in the second little-endian 64-bit header field. A sixth case checks
the ready and shutdown tokens. They do not execute a daemon or replace the
original Python tests.

Reviewed original SHA-256 pins are
`d1fb549b84b3182639191a86988516baf10a8fc1a28758e046e25077d64f03f4`
for `paramsd_startup_gate.py` and
`2c289464407f46382af24d01ab9459ef3e72178d0b00caac1ef8e295437701d9`
for `test_paramsd_atomic_gate.py`. The native gate is explicitly registered
behind `paramsd-startup-gate-native`; the estimation cloud job retains the
original pytest invocation and adds the six native tests and strict Clippy.
This is source preparation and has not yet run in cloud.

`rust/tools/check_paramsd_daemon.py` is the live consumer of `StartupGate`.
It passes only the ready-write and release-read descriptors to the child via
Python `pass_fds`. The current native process owner has no equivalent scoped
pass-FD operation. Both Python files remain in place; no native daemon lane,
`build_library` replacement, or retirement is claimed. Cloud validation must
run the independent original five tests alongside these Rust tests before a
future live original/native comparison.
