# Bridge checker retirement preparation (#296)

The original Python controller `rust/tools/check_bridge.py` remains present and
continues to run in the startup-services cloud job. At source commit
`edb42bf095d6a3e3b38cc0a4546976dfb710363e`, [job 118816129850](https://github.com/bin9208/openpilot-rust/actions/runs/39477200877/job/118816129850)
passed the native controller tests, strict Clippy, and the live Python/native
comparison. Artifact `12290810936` (`rust-startup-services-evidence`, archive
SHA-256 `5072abdcc7fd8ded88212ccdd948969b98d10111728e351f7888bdc741a34edd`)
contains the original `bridge/result.json` (SHA-256
`cdd891ddefa786b776b8e157246625fb879671d146025e9d5e4cca45c366e2e8`),
source scenario records, and stdout/stderr captures.

The [retained observation](../../rust/crates/dev-tools/tests/fixtures/bridge-controller/original-observation.json)
copies only that original controller's report and source-bridge observations.
It records the original checker hash, raw report, scenario and stream hashes,
the two direction digests, and exact Actions origin. It contains no output from the
candidate Rust controller. The fixed observation asserts the original
160-packet direction digests, while the live native controller still runs the
actual private-network bidirectional traffic and compares source/native
bridges. The fixture is not a substitute for runtime namespace or lifecycle
evidence.

Only the exact historical checker file may be absent; its parent directory
must exist throughout the run. If present, it must be
a regular file with SHA-256
`17d5fc9f1d5dc3c59306ba0a8fdcc3bd85b216b362fbbe18fdc7e5daaa246821`;
the held file is checked again after traffic and publication. Missing bridge
daemon binaries and worker inputs have no absence exception. A missing
historical checker that reappears before the final check rejects success.
The native provenance records the historical source digest and the retained
observation digest separately from hashes of executing binaries.

The original/native/worker executable identities remain held through final
stdout, receipt publication, signal-handler teardown and the last commit check.
The late-publication repair is tracked as [#427](https://github.com/bin9208/openpilot-rust/issues/427).
Focused tests mutate a copied executable during final stdout and after receipt
creation; both paths must reject success, with the latter removing only its own
receipt inode. The retained fixture is sealed by a source-pinned hash; runtime
validation of that hash does not itself authenticate the remote Actions archive.

Cloud acceptance must run the present-path live original/native gate, then a
separate absent-path run of the **same native checker ELF** using a private
`OPENPILOT_RUST_ROOT` with no `rust/tools/check_bridge.py`. Compare both
`result.json` reports to the sealed observation and each other; retain exact
ELF SHA-256, full original/native artifacts, namespace/process cleanup and
strict Clippy evidence. The focused identity test exercises changed content,
symlink/FIFO, missing parent and absent-path reappearance negatives. Keep the original Python controller and
workflow until that cloud evidence is accepted. No local build, functional
test, namespace probe, or daemon execution was run for this preparation.

Parent artifact review independently recomputed the downloaded archive digest
and compared the retained raw original report and both source streams to the
exact ZIP members. The current workflow prepares a PATH containing only hashed
native `unshare` and `ip`, then traces the same checker and two bridge ELFs in
the absent-path comparison. The required reference test accounts for all 17
exec attempts: checker/main plus four workers, four unshare calls, four ip
calls and two calls to each bridge. Unexpected or opaque exec attempts fail.
This new absence/exec gate and the #427 repair still await cloud execution.
