# Isolated bridge comparison (#153)

Tracks [#153](https://github.com/bin9208/openpilot-rust/issues/153), found during
[estimator integration #147](https://github.com/bin9208/openpilot-rust/issues/147).

At `22438186`, startup-services
[job110241146692](https://github.com/bin9208/openpilot-rust/actions/runs/36822542683/job/110241146692)
failed in the first original-source outgoing bridge case. Its captured stdout
reported `Failed to create ZMQ publisher for [sendcan]: Address already in use`.
Rust had not run yet. Source service ports overlap the host's ephemeral range,
so other test traffic can prevent binding. The artifact does not identify which
earlier socket occupied the CI port.

The comparison now runs each original/native incoming/outgoing scenario in a
fresh network namespace. A child verifies it differs from its parent's namespace
before enabling its own loopback interface. Regular local users use a private
user namespace; CI invokes the selected Python interpreter with sudo so network
namespace creation does not depend on unprivileged user-namespace policy.
Namespace failure remains a test failure. There is no host-network fallback.

All existing runtime assertions remain: source/native packet hashes and ordering,
160-packet bursts, substring whitelist, lazy shared-queue activation, disconnect
and reconnect, executable mappings and zero-exit shutdown. The bridge runtime and
original source are unchanged. Namespace command/results are retained alongside
each scenario. A timed-out namespace worker and its child process group are
terminated together.

## Reproduction

An owned parent network namespace deliberately listened on sendcan port50650.
The old comparator failed with the same source bind error. With the listener
still present, the repaired comparator passed all four scenarios in separate
child namespaces. The fixture verified its parent differs from the real host
before any loopback or socket operation. Captures are retained under
`.analysis/scratch/2026-10-01-rust-bridge-namespace/{red-collision,green-collision}/`.

The initial reproduction attempt had an incorrect Python import order and is
retained separately; the collision RED/GREEN runs use the built original msgq
binding before the checkout in PYTHONPATH. Those are the meaningful comparisons.
Ruff, CI-policy tests and whitespace checks are additional static evidence.

Exact-SHA Actions and post-merge results remain parent gates. No vehicle, C3X,
NAS, account, host forwarding setting or production process selection changed.

Docs-Not-Needed: isolated host comparison repair; no runtime or setting change.
