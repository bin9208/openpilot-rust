# Independent original Athena VisionIPC builder captures

The original `rust/tools/build_athena_vision_peer.py` has SHA-256
`25373e966c948a5dfce5ee301305771f4f8fe4dcac6a2296af53a9fd4146cc8a`.
`original.json` retains 30 independently executed command, stream, path and CLI
cases. They cover default/custom CXX as a single executable, ignored CXXFLAGS,
all ten C++ source positions, relative/lexical/symlink output paths, overwrite
and partial artifacts, failed compiler/spawn, help/terminator behavior and
parse/runtime exit codes. Unicode negative-number cases distinguish Python's
decimal `\d` from broader numeric categories. The exact root-filesystem disk
floor is tested before output creation.

Fresh original/native C++ executables matched the complete SHA-256
`ad6297d65986c9935923e6478691a8daf0c415646cd5da83e8350c6985887a25`;
their stdout and stderr also matched byte-for-byte. Capture and comparison
evidence are indexed in private scratch under
`.analysis/scratch/2026-10-15-athena-vision-builder/`.
No freshly built peer, Athena daemon, network namespace or vehicle operation
was executed by these builder comparisons.
