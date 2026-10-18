# Version source contracts

These are independently captured original expectations, prepared before native
Version reference implementation. The independent source foundation now passes
eight tests: all 171 original schedule calls, 13 protocol controls, source-pin and
corruption checks, and 21 additional path/descriptor controls. Its original
missing-API RED and three additional admission failures were observed before their
respective implementations. Neither original checker is retired by this foundation.

`reference-original.json` retains all 171 calls from the unchanged original checker
fixture schedule. Only its generated temporary root in request strings is replaced
by `@FIXTURE_ROOT@`; the raw original response strings, floats, errors, and logs are
unchanged. The Rust fixture driver creates real temporary Git repositories and
performs the original filesystem/Git mutations at their original call boundaries.
Each scenario receives an independent persistent source session. It must preserve
the original cache identity, argument arity, default/process cwd distinctions,
exception recovery, arbitrary metadata JSON types, and property failures.

`protocol-original.json` preserves 13 actual original worker controls, including
fatal unknown operation versus caught request errors and persistent recovery.
Empty input is a transport EOF, not an empty JSON request. Original missing checker
argv fails before evidence creation; that observation is saved for the later CLI
test, rather than represented as a source-policy request.

`collector-source-original.json` is solely the original missing-build metadata
response. Historical collector binary packets and disk records were observations
and are not expected-policy fixtures. Fresh current-source Msgq/ZMQ/Capnp/disk
replacement-controller proof is required later. The parent also ran the unchanged
original paired checker against a freshly built current candidate (171 comparisons,
PASS) and its real collector check (PASS). Those current baseline manifests are at
`current-original-reference1/manifest.json` and
`current-original-collector1/manifest.json` in the ignored Version scratch; they do
not mean the replacement native controllers have been implemented or verified.

`provenance.json` binds the immutable expectation files, CPython capture receipt,
five live checker/source inputs, and the current tombstoned loader consumers.
The complete original raw process/Git captures remain private ignored scratch at
`.analysis/scratch/2026-10-19-version-tools/`. No native runtime-version API or
candidate output produced any expectation here.

`path-admission-original.json` separately binds 21 unchanged-original controls:
eight empty-string/dot path and Git cases; eleven real directory/file/pipe/bool
descriptor, permission, C-int boundary and subsequent-cwd cases; and two uncaught
descriptor overflows. The admission RED kept all four foundation tests passing and
failed all three new tests. The repair passed all seven tests (6.86 seconds).
Strict Clippy identified only a new test's non-octal zero permission literal; after
the equivalent `0o0` correction, scoped lib/test strict Clippy passed in 5.13 seconds.
Exact evidence is in the ignored Version scratch `admission-{red,green}.log` and
`admission-clippy-final.log`. No original expectation or tolerance was changed.

`initial-cwd-rename-original.json` preserves an unchanged original worker
handshake, an external rename of its initial cwd, and a successful relative version
read. The new native regression failed before the constructor retained an owned
`O_PATH` directory handle. With that minimal repair, all eight tests passed in
7.55 seconds and scoped lib/test strict Clippy passed in 4.92 seconds. Original
capture bytes remain unchanged; the parent process cwd is never modified. Exact
evidence is `rename-{red,green,clippy}.log` in the ignored Version scratch.

The native source API is dynamic: it executes actual Git commands
and reads actual metadata/version/release files. Production fixture lookup and
delegation to the candidate runtime-version crate are prohibited.

Source closure limitation: `tombstoned_reference.py` imports the original
`check_version_reference.load_source` and exposes the actual original Python
classes to both tombstoned and managed-entry QA. Native Version checkers alone do
not retire that helper functionality. Retain and explicitly inventory the loader
until those callers are converted, or relocate its unchanged body as a retained
Python helper with an actual import/process regression. Relocation is partial
retirement, not removal of the complete Python closure.
