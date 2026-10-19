# Bluetooth journal checker: independent source policy, CI pending

`source-pins.json` identifies the retained Python checker, runtime model and
native fixture source. These are source hashes, not invented result captures.
Neither original nor native programs were run locally while writing this unit.

The native controller translates the pinned original's 721 operations and
CommandWriter policy without importing/linking the candidate writer. Input
includes the 300-send two-channel saturation sequence, the 0.4-second boundary
and both adjacent binary64 values, time reversal, hold replacement, empty holds,
repeat flags, address removal, explicit publishes, and 200 interleaved
send/prune pairs. Its synthetic reference session and inode revisions encode
only the original's normalized comparison boundary; they do not claim to be
actual Python UUIDs, inodes, or execution.

The cloud test runs the **unchanged** Python checker against a fresh native
`journal_fixture`, reads only `original.jsonl` as the expected trace, compares
all 721 normalized steps and the full JSONL input schedule with this translation,
and then runs the new controller against the real fixture. It emits the actual
normalized original trace and source/hash receipt under
`BLUETOOTH_JOURNAL_EVIDENCE` for review and later immutable fixture sealing.
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

All execution/compilation/Clippy outcomes remain pending GitHub Actions.
Local work was source reads, edits, rustfmt/diff and JSON syntax checks only.
Keep check_bluetooth_journal.py live until cloud comparison is reviewed. A later
single-helper retirement must pin the actual independent normalized source
fixture and allow only that checker's exact absence, still rejecting changed,
dangling/reintroduced helpers and missing runtime sources. No deletion,
device validation or performance result is claimed here.
