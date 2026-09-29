# Rust VisionIPC boundary

Issue: [#14](https://github.com/bin9208/openpilot-rust/issues/14). Implements the
external transport boundary in [design.md](design.md), under #1/#5/#6.

Rust owns the native client and each received frame borrows that client mutably.
Reconnect, receive and drop are therefore unavailable while a frame is held.
The existing msgq library continues to own FD transfer, mappings and ION cache
operations. Camera memory can change independently of a Rust borrow, so no
Rust slice into that memory is exposed. Copying into caller-owned storage gives
the CPU model an ordinary owned input. A copy is not an atomic camera snapshot;
the source transport has no producer lease. GPU imports are a subsequent model
integration task and must retain the mapping until GPU completion.

## Implementation and checks

- [x] Extend the existing msgq crate with a typed stream enum, frame metadata,
  single-attempt discovery/connect, bounded receive timeout and frame copy API.
  Keep native code behind `native-skip-miri`; keep clients confined to one thread.
- [x] Add tests first using an original native VisionIpcServer child process.
  Verify missing server, all stream identifiers, padded NV12 bytes/metadata,
  queue and conflate behavior, timeout, reconnect after server identity change,
  copy-size rejection, owned bytes after drop and repeated reconnect cleanup.
- [ ] Build the original host allocation path by default and original ION path
  with an explicit feature. Cross-compile ION without running on the host.
- [x] Run focused tests, workspace fmt/clippy, existing msgq tests and ASan/UBSan
  on the real FFI and native peer. Native FFI cannot run under Miri; it remains
  feature gated and needs sanitizer evidence.
- [ ] Obtain independent review, exact-head CI, merge to dev and post-merge CI.

The native library assumes a trusted local VisionIPC server and may abort on
malformed protocol messages. This boundary preserves that existing trust model;
it is not a parser for adversarial servers. No Python process runs in this API.
Single-attempt connection disables retries, but the original Unix socket
handshake still blocks while waiting for a responsive trusted server.
Production process selection and device state are unchanged by this increment.

## Host evidence (2026-09-30)

- RED: the new test failed to compile because `VisionClient`, `VisionStream`
  and the native peer were absent. GREEN: the actual native-server scenario
  passed, including 30 repeated create/connect/reconnect/drop cycles without
  FD growth and rejected odd-width NV12 metadata followed by recovery.
- `cargo test -p openpilot-msgq --locked`: both existing transport tests and
  the new VisionIPC scenario passed. Source server and client remain separate
  processes, matching the original shared-memory ownership boundary.
- `python3 tools/check_msgq_sanitizers.py --target-dir <scratch>/asan`: both
  test executables and their native peers passed with ASan/UBSan and leak checks.
- Workspace Clippy, formatting, changed Python Ruff and six CI policy tests passed.
  Native source warnings are pre-existing unused parameters, signed comparisons
  and aggregate initializers in the pinned msgq library; they are not suppressed.
- The original ION implementation compiled on the host with `visionipc-ion`;
  it was not executed. Local GNU aarch64 compilation could not start because
  the cross compiler is absent; the new Actions step installs that compiler
  and performs the actual cross build. Target execution remains unverified.
