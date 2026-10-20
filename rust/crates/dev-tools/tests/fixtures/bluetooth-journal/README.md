# Bluetooth journal checker: independent original and absence preparation

`source-pins.json` identifies the historical Python checker, runtime model and
native fixture source. The historical checker remains in this source branch
until the same-ELF absence proof succeeds. No original or native program was
run locally while preparing this retirement.

The native controller translates the pinned original's 721 operations and
CommandWriter policy without importing/linking the candidate writer. Input
includes the 300-send two-channel saturation sequence, the 0.4-second boundary
and both adjacent binary64 values, time reversal, hold replacement, empty holds,
repeat flags, address removal, explicit publishes, and 200 interleaved
send/prune pairs. Its synthetic reference session and inode revisions encode
only the original's normalized comparison boundary; they do not claim to be
actual Python UUIDs, inodes, or execution.

The original cloud test ran the **unchanged** Python checker against a fresh native
`journal_fixture`, reads only `original.jsonl` as the expected trace, compares
all 721 normalized steps and the full JSONL input schedule with this translation,
and then runs the new controller against the real fixture. It emits the actual
normalized original trace and source/hash receipt under
`BLUETOOTH_JOURNAL_EVIDENCE` for review and immutable fixture sealing.
Negative controls mutate that original trace, not candidate output.

Preserved assertions: ordered operations/channels/events and every journal/event
field, mode0600, at most64events and a corpus maximum64, lowercase32hex session
and positive-decimal sequence syntax, one session per side, every per-channel
inode rewrite decision, exact final cruise.json/lane.json names and no temporary
files, original30-second child timeout, nonzero/decoding/cardinality failures.
Generic Python JSON equality retains its numeric/bool semantics; no float
precision tolerance is added.

Seven test entry points also cover mutable/retargeted/FIFO identity rejection,
raw Unix argv, broken output and late cancellation publication, preservation of
a foreign report, leader-only SIGINT and the actual30-second timeout with
acknowledged private child/descendant identities and fallback cleanup.

The independent original/native corpus passed on source
`9845db2db390bc904401a3794b723b510e3bef84`, [Actions run
39507901490](https://github.com/bin9208/openpilot-rust/actions/runs/39507901490),
job `118921359333`, public artifact `12304713010`. The downloaded ZIP SHA-256
is `a482e760a8aa8ba41d6c29cf16e029915e48ef654be2a6c5fd778c416ad082a3`.
`original-observation-39507901490.json` contains only actual unchanged-original
receipt hashes and the ordered SHA-256 of each of 721 independently normalized
original rows. The full raw original (`5,535,468` bytes, SHA-256
`7e56f11a3b729193f4682e9448ea0484f8fd215cc5274de8b0ebc4a38dc30800`)
remains in that public Actions artifact, not in Git. The 721 normalized lines
were verified against their whole-file SHA-256
`d59f9ada5edea294fe825c7c89052212e2766202ec5dfb29ec7aa34d84b9a201`
before the per-row digests were extracted. Thus the compact fixture still
compares every field and per-operation rewrite decision, rather than only a
summary.

The pending same-ELF proof runs a fresh unchanged-original lane, then the
native checker once with the historical source present and once with exactly
that source absent. Fresh UUIDs/inodes can change the raw original hash, so
the test compares every independently normalized row digest. The controller admits
only this one missing path, verifies absence again through publication, and
continues to require the pinned live model and fixture source. The reference
test requires both fresh outputs, the original output, present/absent roots,
both `strace` logs, and before/after checker and fixture digests. It compares
all 721 normalized outputs and exact successful executable/argv pairs. Parent
owns the GitHub Actions wiring and will retain the Python file until this
absence proof passes. No post-deletion acceptance is claimed yet.

The focused test is `cargo test --manifest-path rust/Cargo.toml -p
openpilot-dev-tools --features bluetooth-journal-native --test
bluetooth_journal_reference --locked` followed by the matching strict Clippy
target. Set `BLUETOOTH_JOURNAL_ORIGINAL_OUTPUT`,
`BLUETOOTH_JOURNAL_PRESENT_OUTPUT`, `BLUETOOTH_JOURNAL_ABSENT_OUTPUT`,
`BLUETOOTH_JOURNAL_PRESENT_ROOT`, `BLUETOOTH_JOURNAL_ABSENCE_ROOT`,
`BLUETOOTH_JOURNAL_CHECKER`, `BLUETOOTH_JOURNAL_FIXTURE`,
`BLUETOOTH_JOURNAL_PRESENT_EXEC`, `BLUETOOTH_JOURNAL_ABSENT_EXEC`, and
`BLUETOOTH_JOURNAL_ARTIFACTS_BEFORE`/`_AFTER`. Each artifacts JSON is a map
from the checker's and fixture's absolute paths to their SHA-256 values.
Capture each native CLI with `/usr/bin/strace -f -qq -s 65535 -e
trace=execve,execveat -o "$...EXEC"`; the test requires exactly the checker
and fixture to execute successfully in each lane. The absent private source
root contains the two pinned live sources, an existing `rust/tools` directory,
and no historical checker entry. Both lanes use the same two unchanged ELF
paths. Preserve the complete original/present/absent output directories and
trace files as cloud artifacts, including failure cases.
