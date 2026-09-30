# Native messaging bridge

Issue [#121](https://github.com/bin9208/openpilot-rust/issues/121), full runtime
tracker [#1](https://github.com/bin9208/openpilot-rust/issues/1).
The unchanged MIT sources are `openpilot/cereal/messaging/bridge.cc`,
`msgq_to_zmq.cc` and `bridge_zmq.cc`; the implementation is
`rust/crates/bridge`.

The native binary preserves the source CLI: two or more arguments select incoming
ZMQ-to-msgq using the first address and second substring whitelist; otherwise
outgoing msgq-to-ZMQ binds all service ports. Ports use the original 64-bit FNV-1a
mapping. Incoming subscriptions remain non-conflated with 500ms maximum reconnect
interval and 100ms polling. Outgoing forwards at most 50 queued messages per
ready service, retries EINTR sends, and opens a queue only while TCP clients exist.
SIGINT, SIGTERM and SIGPWR request shutdown.

One Rust thread owns both outgoing monitors and queues. It services monitors
before the original 100ms queue poll and 1ms yield; idle monitor polling is 1000ms.
This avoids cross-thread FFI ownership while retaining connection-driven queue
lifetimes. Activating one queue does not reopen or discard another queue's backlog.
The existing msgq boundary continues to reject invalid empty/oversized payloads;
the original msgq receiver itself asserts on zero-length records.

The source C++ binary is compiled by `rust/tools/build_bridge_reference.py`
against unchanged source and the same native libzmq dependency. The focused
`check_bridge.py` gate uses owned local TCP peers and isolated msgq namespaces.
Both source and native passed two-service outgoing and incoming 160-packet binary
bursts, substring-whitelist selection, no-clients idle operation, client closure,
reconnection, independent service retention and SIGTERM shutdown. Packet hashes
match in both directions. Original pyzmq `disconnect()` did not promptly close
its TCP connection in the first fixture; the lifecycle gate closes the client
socket and reconnects a new peer, observing queue mappings in the bridge process.

The queued transport test also checks inactive/index rejection, independent
activation without lost packets and reactivation. It is included in the existing
ASan/UBSan transport Actions gate. Focused clippy and queued tests pass locally;
complete workspace, sanitizer and generic ARM checks run in Actions.

libzmq is an external dependency under the existing
`rust/crates/logmessaged/licenses/` notices. Original msgq remains a temporary
native transport dependency. This increment does not select the production
daemon, deploy a candidate, contact a device or establish performance savings.
Full startup/upload and actual AGNOS/device acceptance remain pending.

Docs-Not-Needed: native internal bridge candidate; no user setting change.
