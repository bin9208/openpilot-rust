# Native media provider staging (#296)

`openpilot-dev-tools` now supplies `stage-runtime-media`, replacing
`rust/tools/stage_runtime_media.py`. The command retains the provider layout,
FFmpeg/SDL/raylib/libvpx selection, required font, licenses, aliases and source
hash provenance. It stages external native dependencies; it does not execute
codecs or establish hardware behavior.

The native command reads passed provider reports, rejects contradictory hashes,
checks actual AArch64 ELF headers, preflights destination conflicts and directory
symlinks, and freezes bounded source copies with matching hashes before publishing.
Disk admission retains 25 GiB plus the actual estimated staging growth. Existing
matching artifacts must retain source permissions; no Python interpreter or
Python source parsing is used.

Validation on 2026-10-14:

- Initial RED: the native media module did not exist (E0432).
- Five initial filesystem regressions passed: complete/idempotent staging;
  failed/missing/contradictory evidence; mutated/wrong-architecture providers;
  destination file/alias/provenance/directory conflicts; escaping provider aliases.
- An independent review found that matching bytes with mode 0644 could admit a
  non-executable `ffmpeg`. The new test reproduced successful admission before
  the fix. Preflight and post-publication now require source mode equality;
  all six tests pass. Wrong-mode existing files are preserved and rejected.
- Before deleting the original script, an owned synthetic provider fixture was
  staged by both implementations. All 21 file/link/provenance entries matched
  in bytes, modes, alias targets and parsed provenance; JSON stdout matched.
  Native execution used an empty PATH, and a second native run was identical.
  Files/aliases in provenance are sorted by destination in the native output;
  whitespace/key ordering is not the content-identity contract.
- Strict all-target Clippy for `openpilot-dev-tools` passed. Builds used the
  coordinated target cache, two jobs, incremental compilation disabled and more
  than 380 GiB free against the reserve requirement.

Existing native media validation separately passed QEMU/AGNOS provider checks
in `rust-runtime-module-audit-296.md`. This stager's fixture evidence is not a
new provider build, final full-runtime seal, normal device boot, recording/upload
acceptance or measured CPU improvement. Developer Python retirement remains
incomplete; the original comparison script was used only during migration.

Docs-Not-Needed: internal build tooling; no runtime setting changes.
