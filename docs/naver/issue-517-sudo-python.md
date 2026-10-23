# Selected Python across privileged CI comparisons (#517)

At PR500 source `10919538d31b04d28d9a23b383d925441434d579`,
[startup-services job120220822631](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/job/120220822631)
passed dependency installation, native builds and the LPA comparison, then
stopped at `check_bridge.py` importing `zmq` after sudo. Its actual console
records `pyzmq==27.2.0` installed under action Python3.12.15 and that interpreter's
library directory in `LD_LIBRARY_PATH` immediately before the privileged step.
This is a CI execution failure, not demonstrated bridge packet inequality.

The [sudo environment documentation](https://github.com/sudo-project/sudo/blob/main/docs/sudoers.man.in)
describes dynamic-loader environment filtering before sudo starts. Keeping
`-E` alone cannot restore such a removed variable. The candidate passes the
calling shell's library-path value as an explicit argument to `env` after sudo,
alongside the existing Python import path and selected interpreter executable.
The bridge and Athena comparison commands share this boundary. No sudo policy,
dependency pin, project runtime, network isolation or comparison assertion is
changed. No preload variable is added. This is separate from #506's provider
export overwriting the inherited path before a normal Python process starts.

The independently reviewed operational source
`7debe6f1c1eec2dcccb9daa6645277aae2fff13d` has a registered
[normal/legacy/candidate identity probe](https://github.com/bin9208/openpilot-rust/actions/runs/39910424061).
It records actual version, executable, prefix/base-prefix, UID, loader path and
four installed module identities/import errors. The candidate must retain the
normal interpreter/import identities while running as root. The unchanged
legacy result is retained as observed; reproduction is not fabricated.
The probe installs no dependencies and invokes no project code.

The real probe passed. Artifact12481924401 SHA-256
`28f6a50016ff68d70029d530d8d423d8523f297ec51de7b73829f499a135bad8`
retains normal, unchanged legacy and candidate receipts. The executable and
prefix stayed the same in all three. Normal Python reported3.12.15 and imported
all four modules. Legacy sudo removed the library path, reported3.12.3 and
failed all four imports. Candidate sudo restored3.12.15, the exact normal module
paths and successful imports while retaining UID0. There is no claim that the
prefix itself changed or that a complete bridge/Athena comparison ran here.
Full bridge/Athena comparisons, exact-source required gates, normal dev merge
and separate post-merge checks remain pending.
Local checks are YAML/Bash syntax and diff review only, with no functional
execution, project imports, dependency installation or device operation.
