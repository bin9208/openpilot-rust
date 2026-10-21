# Original-only beep reference observation

The unchanged `rust/tools/check_beepd_reference.py` runs 20 policy scenarios
against both the original `beep_source.py` and the actual Rust `beep_trace`
example. The capture gate first requires that live pair to pass, then retains
only each original result and its raw stdout, stderr and trace. It records the
checked-out source and all input hashes. The candidate's results never supply
expected rows. The exact capture recipe is in ignored
`.analysis/scratch/2026-10-21-beep-reference-capture/README.md` until the first
cloud artifact is accepted.

The native controller requires `--original` to name the captured directory and
`--original-manifest-sha256` to pin its `files.sha256` manifest. It verifies the
full 82-file roster and member bytes before execution and again before success.
The root original source hashes are pinned in `beep_reference_check.rs`. The
Rust example and original Params binding are verified before and after the
comparison. Policy cases cover every duplicated alert in the source schedule,
absent/empty/prefixed/mutated volumes, command and sleep errors, worker reports,
the explicit original SIGABRT/native error exception and Ratekeeper catch-up.

The hosted integration test requires `BEEP_REFERENCE_BINARY`,
`BEEP_REFERENCE_BINDING`, `BEEP_REFERENCE_ORIGINAL`,
`BEEP_REFERENCE_ORIGINAL_MANIFEST_SHA256` and a fresh nonexistent
`BEEP_REFERENCE_EVIDENCE` path. The parent feature registration needs
`beep-reference-native` to enable `dep:zmq`; register the
`check-beep-reference` binary and `beep_reference_check` integration test under
that feature. Run the module tests, the real 20-case integration test and strict
Clippy in Actions. Preserve the existing Python comparison until the capture,
native run and later same-ELF absence proof are accepted.

No Python source is retired by the reference-controller preparation. The real
daemon/controller migration and device behavior remain separate work.

## Preserved independent capture

The20original rows and their raw streams/traces are now stored in `3835653-original/`:82members plus `files.sha256`, manifest SHA256 `0a02d6aacb5b930012a47886c389571d9cf3ff404ecd796171045f10734a9188`. They are a data-only extraction from pre-implementation [run39524132398/job118974708445](https://github.com/bin9208/openpilot-rust/actions/runs/39524132398/job/118974708445), artifact12312876978 (357,047bytes, ZIP SHA256 `8a63fbda38722e6ab9d39a4a03359f86a0830b5fa0170a45a7966f36650d67f9`). The actual synthetic merge source was3835653ecdea12c08f0cb9cb2807b0a493f20bb7 for PR head8e8b1e8f. Original binding provenance independently records that source. All seven current original source files were compared byte-for-byte with the Git objects at that merge; none changed.

Each saved case JSON contains only the first, original Python result from its paired comparison. All original stdout/stderr and trace bytes are copied verbatim; trace files total467rows. Parent independently verified each raw file against its original row and every packet array is empty. `inputs.sha256` records the seven exact original source hashes and the executed binding hash from its retained build provenance. The old native beep executable hash was not retained by that job and is not invented in this extraction. No candidate output or Python executable/script is copied into the fixture. Fresh native comparison and later actual deletion are pending.
