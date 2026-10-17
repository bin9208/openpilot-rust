# Registration detector archive oracle

`original.json` contains 37 independent captures of the unchanged original
`rust/tools/check_registration_vendor.py`, SHA-256
`23fd3e692a0de5b089f5abe0e7c7068fc31931bbe39f892ca85ab86312eddac4`,
run with pinned CPython 3.12.15. Each capture uses real temporary package files,
real tar archives and the unmodified original command body. The outer gzip hex
is storage compression of exact original archive bytes; tests decompress it
before passing the archive to the native checker. No expected result is
produced by the Rust implementation.

Cases preserve archive ordering, duplicate-member update order, normalized and
rejected paths, directory handling, regular/NUL/contiguous/GNU-sparse file types,
rejected links/FIFO members, local symlink behavior, Unicode and raw Unix filename
bytes, missing/extra/changed files, sole-edit and repeated-substring rules,
checksum/provenance failures, output creation timing and truncated archives.
Raw tar, gzip, bzip2 and xz inputs all have independent successful captures.
Nothing is extracted from an archive into the filesystem.

Python accepts an empty GNU sparse header whose real-size numeric field is
blank. The Rust tar parser requires explicit octal zero. A narrow compatibility
adapter changes only that field and its header checksum in the in-memory tar
view, after verifying the complete original archive SHA and original header
checksum. Paths and payloads are never normalized by that adapter.

Actual published archive: charset-norm 3.5.1, downloaded from the provenance URL
`https://static.crates.io/crates/charset-norm/charset-norm-3.5.1.crate` and verified
against SHA-256
`bffd389b7f5eb8907ca98f54dd832502f54313b837e5f1a30f28ebb1a180cd7b`.
The unchanged original checker verifies 28 files and exactly one changed file,
Cargo.toml's rust-version metadata. Archive/source comparison evidence and
capture programs are private under
`.analysis/scratch/2026-10-15-registration-vendor/`.
