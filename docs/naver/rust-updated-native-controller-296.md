# Native updater comparison controller (#296)

`check-updated` now compares five updater scenarios against complete independent
original-only JSON. It builds private native Git/sudo/bash fixtures, runs the actual
updater trace through the owned process launcher, preserves rollback bytes and a
held transaction-lock inode, and checks all ordered command/snapshot fields. Only
the previously documented exact obsolete-staging removal difference is allowed.

The old controller, source probe and16-case pytest file were removed together.
All16 original comparator and held-lock cases are retained by the Rust suite;
11 independent CLI admissions, source drift/reappearance/symlink rejection and
failed-run receipt controls remain mandatory. `updated_fixtures.py` still serves
the original daemon comparator and remains explicitly pending.

Before removal, source505cc30c520ddb4a0bbbd3fbe82e7cd8c2584eb3 passed
[run39543788680](https://github.com/bin9208/openpilot-rust/actions/runs/39543788680),
job119037266203, workflow3ef960060b7bbb98266b84241e287ef566b324f4.
Artifact12320896721 ZIP SHA256:
`ebfe3e6294e23cc5467c4c89f1ebefd45d9d63f46d3106a8f90f9d95930b50f5`.
It contains16 original tests,15 native tests, fresh five-scenario equality,
strict Clippy and two1,089-execution audits. Each successful executable is
classified; failed/unfinished/opaque calls are rejected. Input seals match before
and after. The private admission root omits only the retired historical pair;
five other original dependency pins still refer to the checkout and the retained
fixture is copied into that root. This is no claim of a wholly Python-free tree.

Actual deleted-checkout cloud proof and the complete updated startup-services job
remain pending. No device, network uploader, AGNOS installation or driving result
is established by these host fixtures.
