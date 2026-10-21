# Synthetic Panda device original observations

This fixed test fixture contains synthetic protocol requests and observations,
not vehicle captures, keys or settings. It preserves the unchanged original
Python checker's independently generated full 1,746-request/2,490-operation
corpus, C++ responses and 16 CLI/filesystem observations. Historical native
responses are retained only as an additional raw-byte regression record; the
independent C++ responses remain the behavioral oracle. No Python source or
executable is packaged, restored or executed by the native materializer.

Provenance: repository `bin9208/openpilot-rust`, source
`7fc4698e3d4564a6bd8499ec98a80704f3cd9c77`, Actions run `39631590479`,
artifact `12359646845` (`panda-device-original-only`). The accepted source commit
is recorded inside the immutable capture manifest, independently of the
one-time diagnostic workflow commit. The borrowed C++ ELF originated from run
`39616860491`, artifact `12352894540`; its build/source provenance remains in
`builder.json`. The ELF itself is not part of this fixture.

Original ZIP SHA256:
`b454a1c91ec7c0856f56b5e5a0216f5d4e366154c162c1540bcd35d529ba6717`

Original capture-manifest SHA256:
`ff7b197b3eca5408abbf7dee5b8df196a8fdd5ea61b4e394e829ca382428ea8d`

`observations.tar.gz` SHA256:
`f1c8330d973928bc5c959b622eb3e819b276d422280788961073b6ccb7e3eaa9`

`fixture.json` SHA256:
`c0e8530ec840ffb62d70a418e3a17ca4815c53dcafd3e003bdbbd3c7d68c6594`

The deterministic gzip/tar blob is 2,333,895 bytes; 35 regular members expand to
14,321,241 bytes: exactly 33 sealed observation files, the capture manifest,
and the borrowed builder manifest. Tar member names are sorted, timestamps and
owner IDs are zero, modes are 0644, and gzip timestamp is zero. Original file
bytes are unchanged. `fixture.json` lists the complete size/hash roster.

`stage-pandad-device-reference --root ROOT --output ABSENT_DIRECTORY` verifies
the pinned blob, complete membership and hashes, original seals and all 14
current C++ source pins. It stages atomically with no replacement and a shared
25 GiB plus bounded-growth reserve. `environment.env` and `environment.json`
record the absolute staged paths and external original manifest pin for the
maintained reference tests. Staging performs no historical program execution
and needs no remote artifact service or Python installation.

The observations derive from the repository's MIT-licensed original tooling
and synthetic C++ protocol implementation. The original copyright and license
are retained in the adjacent LICENSE file. Source hashes and original source
provenance remain in the authenticated capture manifest; those hash records
are not historical source restoration.
