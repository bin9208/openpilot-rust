# Panda padding QA finalization (#430)

Issue: https://github.com/bin9208/openpilot-rust/issues/430
Track: #296, PR #343 into dev. This is controller evidence integrity; it does
not change the Panda runtime, malformed-input policy or device transport.

Source review found that `execute()` verified its source and probe identities
through stdout and report publication but dropped those descriptors before
returning `Completed`. The CLI calls `Completed::finish()` again after signal
handler teardown; that method previously checked only cancellation. A mutation
in this interval could leave an accepted report with stale evidence bindings.

`Completed` now retains the source root, all admitted source descriptors and both
probe descriptors, and rechecks them at every finalization. Failure removes only
the report reserved by its still-open descriptor. A foreign replacement survives.
The historical checker's initial present/absent state must also remain stable.

The standalone `pandad_safety_padding_finalize` regression replays independently
captured original responses through the public controller interface. The old
`edb42bf095d6a3e3b38cc0a4546976dfb710363e` checkout receives this exact test
and twelve sealed raw captures. The focused baseline must compile first and
then actually fail the named post-execute native-probe mutation assertion.
Compile errors, an external timeout and unrelated failures do not count as RED.
The current gate runs all four mutation/ownership cases and strict Clippy.

The related retirement preparation retains the unchanged Python checker and its
original three-phase controls. It adds actual original/current-native/absent-native
same-ELF comparisons and an interpreter-free trace requiring exactly four exec
attempts: checker, source encoder, source safety probe, native safety probe.
All three ELFs are hashed before and after. The full 21-input source-build map
is pinned to the actual original manifest and revalidated against current files.
The fixture stores raw observations, not a renamed copy of Python source.

Status: source, formatting, syntax and diff review only. The old-RED/current-GREEN,
three lanes and exec audit require GitHub-hosted Actions on the committed head.
No Python deletion or completed runtime/device validation follows from this work.
