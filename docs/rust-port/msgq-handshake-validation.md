# Native interoperability test synchronization

Issue [#47](https://github.com/bin9208/openpilot-rust/issues/47).
The b3384b93 push [failed](https://github.com/bin9208/openpilot-rust/actions/runs/36666484233/job/109734322675)
when the Rust receiver reached its two-second timeout before the native echo
arrived. The native peer independently used a two-second receive timeout, so the
test assumed its processing would finish earlier than the receiver's deadline.

Original `msgq_poll` checks ready queues before calling nanosleep. An arrival
between that check and the sleep can leave the message queued until the full
sleep completes. A test-only LD_PRELOAD shim widened exactly that native
readiness-to-sleep boundary by 100 ms. The unchanged test then reproduced the
same None-versus-payload failure at two seconds. Three hundred ordinary local
attempts had passed. The cloud log lacks the native timestamps needed to prove
that exact interleaving caused its individual failure.

The native test peer now acknowledges a completed echo send on its retained
stdout pipe. Both Rust interoperability tests wait for that acknowledgement,
then require the exact payload to be immediately present with a zero-duration
receive. The runtime-namespace case deliberately delays its echo by 2.1 seconds,
so returning to the old independent two-second assumption is a regression.
All payload, namespace, capacity, exclusivity and process-exit assertions remain.
No production transport, queue size, polling implementation or daemon timeout is
modified.

The four transport tests pass; the forced source-poll interleaving also passes
with the new delayed echo, in about 4.2 seconds. The shim and red/green captures
remain in private analysis evidence. It is test instrumentation and is never
linked into or loaded by a runtime artifact. Cloud checks on the new integration
head and separate post-merge checks remain required.

Docs-Not-Needed: interoperability harness synchronization only.
