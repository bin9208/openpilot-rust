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
original pytest invocation and adds six native behavior tests, one retained-
source identity test, and strict Clippy.
The original five cases and native foundation tests passed in the prior
cloud run identified by the #296 coordinator. The new ignored actual-daemon
test described below has not yet run in cloud.

`rust/tools/check_paramsd_daemon.py` is the live consumer of `StartupGate`.
It passes only the ready-write and release-read descriptors to the child via
Python `pass_fds`. The native gate now uses the repository's existing
`command-fds` child-only mapping to pass owned clones of the same two CLOEXEC
pipe ends. The C hook keeps separate ready-write and release-read FDs, while
the parent retains the exact pose cursor check. No process-wide inheritable
descriptor or caller-written unsafe `pre_exec` is needed; daemon stdin,
stdout, and stderr retain their original roles.

The ignored `native_mapped_gate_holds_actual_paramsd_second_poll_and_shutdown`
test is a first actual consumer: it spawns the real `openpilot-paramsd` with
the unchanged source-built C hook, holds its second poll after one completed
empty update, releases it, and verifies bounded shutdown. Its cloud inputs
are listed in `docs/naver/rust-paramsd-native-gate-296.md`. Both Python files
remain in place. This does not replace the original 1,202-frame source/native
daemon comparison or establish an absent-Python retirement gate.
