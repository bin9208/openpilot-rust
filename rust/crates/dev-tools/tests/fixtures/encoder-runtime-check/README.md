# Encoder runtime source evidence

`wire-original.json` runs the unchanged original Peer.drain against actual wire
messages from the complete current cereal Event schema. Only socket dequeue is
controlled. It preserves every output key, payload/header/thumbnail SHA, original
rejection, raw-message length prefix and successfully decoded prefix before errors.
The corpus was captured before the Rust implementation existed. Source/schema
hashes identify the original behavior; unknown or added schema behavior must be
reviewed when that guard changes. This corpus does not prove live lifecycle behavior.

Additional independently captured units:

- `exercise-original.json`:18 original schedules including every mode, rotation,
  activation/prewarm, lag overwrite, restart and all corresponding assertions.
- `main-original.json`:11 original orchestration outcomes including repeated or
  selected cases, both child phases, disk refusal and mismatched packet results.
- `close-original.json`:5 original cleanup/exception precedence paths, preserving
  artifact order and the resources that remain when a write itself fails.

The wire corpus includes an unknown enum control: the original rejects it while
building to_dict. A numeric fallback must not broaden the admitted message set.

Retirement gate: all ten actual original C++/Rust daemon cases passed with this
native peer, then the entire native CI ran 5 codec, 32 V4L and 10 IPC cases with
no Python executable on PATH. Retired checker absence is permitted only for its
specific source pin; a reintroduced changed source fails. The full wire corpus
has an immutable byte hash and the live cereal schema remains required. Owned
native child tests additionally verify frame validity, all pixels, failed spawn,
failed exit, timeout/reap, IPC namespace cleanup and partial-file failure order.
Device boot/record/upload and target-driver performance are separate acceptance.
