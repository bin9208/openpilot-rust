# Bridge checker retirement (#296)

The original Python controller `rust/tools/check_bridge.py` is removed after
the full original/native and same-ELF absence comparison passed at source
`2d8d027932dc880bd0acd00288b256ac8d7f1b89`.
[Startup-services job 118875506146](https://github.com/bin9208/openpilot-rust/actions/runs/39494388960/job/118875506146)
passed controller tests, strict Clippy, actual bidirectional traffic, both
publication-mutation checks, and the required native exec audit. Artifact
`12297452708` (`rust-startup-services-evidence`, ZIP SHA-256
`ea65e1f32b436f902974a726724aad8a3d5ae0e4404849c2f2f8a7c72f7975e8`)
was independently reviewed by the parent. The raw original report is byte
identical to the earlier original capture; the original, current native and
Python-absent native reports all match the immutable original observation
(SHA-256 `93238454770d40562d60087fa355b2da8cfc72112a56ee751df1fb69098201e3`).
The two 160-packet direction hashes remain
`2e863802da7df834e15445128337e8007dc48bb32f0d5356710feb7bf3aac3ec`
and `4b2e0fd751509172d255cceaad2e2e476ec90890c91baa8c1cc5a80619e318e9`.

The observed checker/original/native executable hashes are respectively
`9052a0dc96df12225cf0b0d2fabf3ba8c12eb9323c3b42982b443c5b8dccb10f`,
`782f45252475ebc9f608d0353679dcc3c894a176a5374658caaefd02fecad937`,
and `bc0fb8f3c39def396ca3c3772e392a2b0ac0a3d26c1ec66c85150960ffbc8088`.
Both live report provenance and the exec audit bind those identities. The
parent independently parsed all 17 exec attempts: five checker entries, four
`unshare`, four `ip`, two original bridge and two native bridge calls.

The post-deletion cloud gate requires the historical source to be absent in
the actual checkout, seals all five executing tools before either traffic run,
compares both fresh native reports with the original observation, and checks
both provenance receipts and all executable hashes afterward. It retains the
original C++ bridge build and namespace/lifecycle checks. This revised gate
passed at checked-out source
`0d1700e5a3ad0181ecfdd7f7f25e4fc9d088e1d2`,
[job118894126434](https://github.com/bin9208/openpilot-rust/actions/runs/39499806776/job/118894126434).
That diagnostic workflow's head was `6acd3681`, distinct from its explicitly
pinned source checkout. Artifact `12299956453`, ZIP SHA-256
`cb5ed1b805fc7c50bb0ba5c6e84ea70e9fe4a9c62e9ee8380e96d481c5bfc81d`,
contains both new traffic reports, provenance, five executable seals and the
17-exec trace. The parent independently downloaded and reviewed these records:
the same controller/original/native hashes and two160-packet direction hashes
remain accepted, with no Python exec. This is a passing post-deletion host
gate, not a passing whole candidate, dev integration or device operation.

Historical original capture: at source commit
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

The pre-deletion cloud acceptance ran the present-path live original/native
gate, then a separate absent-path run of the **same native checker ELF** using a private
`OPENPILOT_RUST_ROOT` with no `rust/tools/check_bridge.py`. Compare both
`result.json` reports to the sealed observation and each other; retain exact
ELF SHA-256, full original/native artifacts, namespace/process cleanup and
strict Clippy evidence. The focused identity test exercises changed content,
symlink/FIFO, missing parent and absent-path reappearance negatives. No local build, functional
test, namespace probe, or daemon execution was run for this preparation.

Parent artifact review independently recomputed the downloaded archive digest
and compared the retained raw original report and both source streams to the
exact ZIP members. The current workflow prepares a PATH containing only hashed
native `unshare` and `ip`, then traces the same checker and two bridge ELFs in
the absent-path comparison. The required reference test accounts for all 17
exec attempts: checker/main plus four workers, four unshare calls, four ip
calls and two calls to each bridge. Unexpected or opaque exec attempts fail.
The absence/exec gate and #427 repair passed on `2d8d0279`; the actual
post-deletion gate subsequently passed on `0d1700e5` as recorded above.
