# Local patch: Web Sound close acknowledgement

This directory is the cached crates.io `tungstenite` 0.29.0 crate, preserving its
original licenses, manifests and `.cargo_vcs_info.json`. Recorded upstream commit:
`c2921718b76f0ed8c69a895397ed981e4a321f6f`.

Only `src/protocol/mod.rs` changes production source. The additional public
`WebSocketConfig::reply_with_normal_close(bool)` option defaults to false. When
enabled for Role::Server, a valid peer Close queues code1000 with an empty reason
through the existing `set_additional`/flush path. Validity is determined before
existing invalid-code sanitization. Client behavior, invalid-code replies and
the returned received Close remain unchanged. Buffer ownership, pending data/PONG
handling, frame parsing and raw I/O are unchanged. No unsafe code or dependencies
are added.

Three trailing spaces in the upstream changelog are removed for repository
whitespace checks. The local Cargo cache marker is excluded from version control.

The original Carrot Web Sound feature replies1000/empty to peer3001/reason and to
empty peer Close. Native unmodified3001/reason mismatch was captured before this
patch. Focused tests are in the consumer's owned
`crates/carrot-server/src/web_sound/close_tests.rs`, using the existing
tokio-tungstenite re-export rather than resolving upstream benchmark dependencies.
They cover default echo versus opt-in, unchanged peer return values, client role,
invalid code sanitization, and completion of a partially written application frame
before the queued Close. Exact loopback feature/process comparisons are retained
under `.omo/evidence/225-web-sound/` in the repository root.
