# Rust process supervision (#84)

- Scope: [issue #84](https://github.com/bin9208/openpilot-rust/issues/84), under
  the approved full-runtime conversion in #1/#6.
- Source: `openpilot/system/manager/process.py`; starting integration revision
  `78ebd63962cb077dcbbb426e024664f53435a348`.
- Implementation: `rust/crates/process-supervision`, including the native
  `openpilot-process-child` launch boundary and focused source-oracle tools.
- Contract and validation commands:
  [process-supervision-validation.md](../rust-port/process-supervision-validation.md).
- Local evidence: `.omo/evidence/process-supervision/evidence.json` in the
  issue-84 worktree records exact revisions, invocations, source/binary hashes
  and real process/state/log/cleanup artifacts. Parent owns Actions and PR
  integration; local host/QEMU evidence is not an exact-SHA Actions result.
- Inherited failed-PID-write duplication remains open in
  [#90](https://github.com/bin9208/openpilot-rust/issues/90).
- Full manager startup/catalog, application managed-entry/crash adoption,
  AGNOS/device acceptance and CPU savings remain separate. No user guide or
  production process selection changes are included.

Docs-Not-Needed: internal optional process lifecycle library preserving current
settings and production startup behavior.
