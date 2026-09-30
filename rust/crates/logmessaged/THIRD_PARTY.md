# Native transport dependencies

The Rust crate is project-owned MIT code. It calls these external transports;
they are not represented as Rust rewrites of the underlying libraries.

- `zmq` 0.10.0 and `zmq-sys` 0.12.0: the MIT/Apache-2.0 bindings from
  <https://github.com/erickt/rust-zmq>. License texts are in `licenses/`.
- `zeromq-src` 0.2.6+4.3.4: its MIT/Apache-2.0 build support builds the bundled,
  unmodified libzmq 4.3.4 C++ sources. Cargo.lock pins the registry source checksum.
  The build works through Cargo's target C/C++ compiler, including cargo-zigbuild
  for static Linux aarch64-musl; it does not require a target system libzmq.
- libzmq 4.3.4: LGPL-3.0-or-later with the original independent-module linking
  exception. `licenses/libzmq-COPYING`, `libzmq-COPYING.LESSER`, and
  `libzmq-LINKING-EXCEPTION.md` retain that notice. Source comes from the locked
  zeromq-src crate, <https://github.com/zeromq/libzmq/tree/v4.3.4>.
- Original `msgq_repo/msgq` code remains behind the existing `openpilot-msgq`
  boundary and `openpilot-messaging::PubMaster`, with its existing source/license.

UUID bytes come from the system randomness API via getrandom; uuid sets the
version/variant bits. Python-compatible shortest float digits use zmij, already
present in the workspace. Their registry provenance and checksums are locked.
