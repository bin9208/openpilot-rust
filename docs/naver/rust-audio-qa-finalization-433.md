# Audio QA finalization identity (#433)

Tracking: [#433](https://github.com/bin9208/openpilot-rust/issues/433), within
the independent #296 conversion. This is a source-ready repair; cloud
behavioral RED/GREEN, complete audio checks and Clippy remain pending.

At baseline `2d8d027932dc880bd0acd00288b256ac8d7f1b89`, the actual soundd
checker verified pinned sources and four executing artifacts after publishing
its root receipt. Returning `Completed` then dropped those bindings. The CLI
calls `finish_cli` before and after removing signal handlers, but both calls
only observed cancellation. A change during that explicit finalization phase
could therefore leave an accepted stale receipt. This is separate from the
#426 blocked-stdout process-test timeout and is not an observed device audio
failure or a claim about arbitrary changes after final return.

`Completed` now retains the source root/map and held daemon, assets helper,
PortAudio fixture and checker identities with their paths. Both final CLI
checks verify the complete binding. The public `run` wrapper also finishes
verification before returning its summary. Failure removes only the receipt
inode owned by that result, preserving a foreign replacement.

Two new unit tests invoke the actual bounded worker through its actual receipt
publication, then exercise unchanged input, a private pinned-source mutation,
a copied daemon replacement, owned receipt rollback and a foreign replacement
receipt. Each unique `finalization433-*` case retains private inputs and raw
worker output under the required `SOUNDD_DAEMON_TEST_OUTPUT`; no repository or
shared daemon input is modified.

The mandatory cloud baseline step checks out exact 2d8 and appends only the
same helper and two tests in a separate test module. It preserves the old
production implementation, builds an old checker in an isolated target, and
runs the exact fully qualified source-mutation regression. Admission requires
exit101 and the specific `accepted changed source at finalization` assertion.
Compilation failure, timeout, missing/zero tests and unrelated failures cannot
count as RED. The job retains the test-only patch, source/binary hashes and raw
log, then removes its owned temporary checkout/cache. Disk reserves of4096 and
512MiB are additional to the existing audio reserves.

Current unit tests run before the complete process suite so that an unrelated
process failure does not hide this earlier deterministic evidence. All existing
process predicates, five-second mutation timeout, original/native audio
comparison and strict Clippy remain required. Local work was limited to source,
format, YAML/shell syntax and diff review; no functional code ran locally.
