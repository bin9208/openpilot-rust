Original Python CLI captures
============================

`original.json` captures all five independently executed original builders from
`30866c90^`; source hashes are in `provenance.json`. `$OUT` is a fresh owned
output directory and `$DEPS` contains controlled header/prefix paths. Compiler
and capnp peers produce fixed products, allowing both successful source-driver
commands and rejected CLI inputs to be observed without devices or compilation.
The tests assert original exit status, compiler invocation count and absence of
filesystem changes on parse failure. The existing Panda oracle fixtures retain
full command/include/manifest equivalence coverage for seventeen modes.

`four-interfaces.json` captures 82 parser-only cases from the original encoder,
Athena vision peer, firmware SPI driver, and msgq sanitizer entrypoints. Exact
source revisions and SHA-256 values are in `four-provenance.json`. A wrapper
calls the original `ArgumentParser.parse_args`, records the returned namespace,
and exits before the original builder/checker can perform any work. These
captures establish CLI admission, help ordering and parsed values; the existing
family fixtures and compiler comparisons establish build/check behavior.
Firmware tests separately supply the native driver's two explicit Python ABI
metadata arguments, which were implicit in the original Python driver.

`five-interfaces.json` preserves 73 original parser captures for the bootlog,
loggerd, Panda runtime, bridge, and JPEG sanitizer commands, with original
source hashes in `five-provenance.json`. Capture uses the same parser-only
wrapper described above. Unit tests compare admissions and supplied parsed
values; `five_cli.rs` runs 25 safe help/error cases and checks exact exit codes
and absence of filesystem changes. Compiler/check logic is covered by each
family's existing command/output/failure fixtures.

The bridge dependency-gate regression additionally ran the full original builder
with a missing `--target` cache and no explicit `--zmq-root`: original
`parser.error` returns 2 before creating the output/target. The native regression
asserts that same exit code, diagnostic and absence of filesystem changes.
