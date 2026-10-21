# Encoder JSON11 filesystem prefix (#454)

The native encoder CI controller parsed JSON with surrogate support but decoded
`prefix` through UTF-8-only text. A valid JSON11 stager result containing Unix
raw0xff therefore failed before the encoder source builder. The original path
contract and the corrected stager preserve those bytes. Tracked in
[issue454](https://github.com/bin9208/openpilot-rust/issues/454).

The candidate extracts Panda's existing canonical surrogateescape decoder into
`python_fs::from_json`, uses it in both callers and adds the shared source to
both provenance inventories. Encoder dependency metadata is still written before
prefix decoding, preserving original failure order. Explicit prestaged prefixes,
disk reservations, command/exit captures and failed-run receipts retain their
existing behavior.

A new owned-process stager regression emits escaped JSON with a raw0xff output
prefix and requires the exact filesystem bytes, dependency metadata, reservation
and command/exit receipts. Shared-codec tests cover all256bytes, Unicode, raw
surrogate UTF-8 sequences and rejected unencodable/noncanonical strings. Parent
and independent worker reviewed the changes; local formatting/diff checks only.
Hosted old-source RED, fixed full encoder/Panda regressions and strict Clippy
are pending. No passing functional or device result is claimed yet.
