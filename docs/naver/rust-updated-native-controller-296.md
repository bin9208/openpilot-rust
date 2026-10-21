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

Actual deleted-checkout proof passed at source083f937a0eda7e05bd84f18bf68b0afce5a5956e:
[run39547256419/job119048126004](https://github.com/bin9208/openpilot-rust/actions/runs/39547256419/job/119048126004),
workflow759963f7387f6e24e3f7989d66b4a52ed8916acb,
artifact12322531729 ZIP SHA256
`c0f5ef8364da8907ae059f36201d45cd9ca31aea222d34b965eee88bd9bf2c6b`.
The actual checkout historical pair is absent; both roots retain1,089 native-only
execs and15 tests/Clippy/default CLI pass. Parent inspected all20 native JSON
across checkout/private/test/default lanes and matching before/after input seals.
Source policy run39547261512 at083f937a passed13 disk,74 isolation,2 lagd and2 timed checks.
The complete updated startup-services job and latest full PR/ARM remain pending. No device, network uploader, AGNOS installation or driving result
is established by these host fixtures.
