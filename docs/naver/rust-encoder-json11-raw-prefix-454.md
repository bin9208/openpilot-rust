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
Hosted evidence below supersedes the original pending status. No device result
is inferred from this host QA controller.


Accepted full [39618350715/job119284926631](https://github.com/bin9208/openpilot-rust/actions/runs/39618350715/job/119284926631)
passed source f5c8ee8de76c038883f65668a45c49ad559ca83b, workflow
5dc3281677cb33a1ac69fc45662303a60c852c63, Rust1.94. It requires the exact
917cd7db caller to fail the raw prefix test, restores fixed source, then passes
6encoder/14Panda/4filesystem tests and strict Clippy. The extended test runs the
actual owned source-builder child, checks all14 argv tokens including its raw
prefix, and verifies both command/exit receipts and unchanged disk reservations.
It preserves metadata-before-decoding failure order and raw0xff without U+FFFD.

The same full job passes source-build, codecs, V4L and runtime commands with
exit0, ten runtime pairs and complete package/IPC regression gates. Parent
audited artifact12354382978 (20,520,518bytes/771members, all CRCs), ZIP SHA256
`003a9c8d0c0d0be4fc96978a4c6eb9d34760b24147de070eb773ebf0e9dba91a`.
Earlier79365a1e full39570075410 is retained as historical evidence; this newer
run includes the previously missing actual next-builder argv boundary. These
are owned host tool/fixture checks, not an on-device compiler/raw-path run or
normal boot/record/upload acceptance.
