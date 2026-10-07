# Native Rust msgq and VisionIPC — issue #194

Status: the Rust transport implementation and component verification are complete;
composed consumer validation, review and exact-head CI remain integration gates.
This stage belongs to the approved full-runtime conversion in
[#1](https://github.com/bin9208/openpilot-rust/issues/1). It is not a normal-startup
candidate or device handoff.

Scope and acceptance are tracked in
[#194](https://github.com/bin9208/openpilot-rust/issues/194). The starting revision is
`51797878b7b6073f8e6ebb6bb26f34c03f7fac0a`; the original `msgq_repo` tree is
`6486b10e859da78a5102c9b36897e992e7fe3240`. Its package metadata identifies comma.ai
and the MIT license. Original source and licensing remain preserved.

## Boundary and implementation sequence

Replace the implementation behind the existing Rust publisher, subscriber,
batch, VisionIPC client/server, image and descriptor interfaces. Original C++
programs remain independent test peers. No production process selection,
scheduling, schema, validity threshold or camera policy changes belong here.

1. Preserve the previous CXX baseline, source identities and actual executable
   evidence. Demonstrate its project-owned C++ runtime symbols before removal.
2. Implement the queue header protocol, ownership, registration, wrap/overwrite
   handling, conflation and polling in Rust. Check original/Rust processes in
   both directions, plus Rust/Rust operation and malformed-contract rejection.
3. Implement VisionIPC Unix sockets, SCM_RIGHTS, byte-level metadata, owned
   mappings, generic allocation and ION allocation/import/cache operations.
   Preserve retained-buffer behavior and the separate restart limitation in
   [#191](https://github.com/bin9208/openpilot-rust/issues/191).
4. Validate cleanup, corruption, startup races, signals, competing publishers,
   reader eviction and cross-process lifecycle. Run pure-memory Miri checks,
   meaningful native sanitizers, target ABI/build checks and consumer integration.
5. Inspect final native symbols/linkage to establish that original C++ transport
   implementations are absent from the runtime artifacts. Record remaining
   external dependencies and exact-revision CI separately.

The queue retains its 984-byte header, forty reader slots, 64-bit atomic header
fields, high-32-bit cycle/low-32-bit offset, aligned length tags and `-1` wrap tag.
The Rust memory boundary uses consistent aligned atomic words for queue memory;
it does not expose shared packet bytes as Rust slices. Pixel mappings likewise
expose checked copies and borrowed descriptors, not a producer buffer lease.

The production crate no longer depends on `cxx` or `cxx-build`; the bridge sources
are removed. `libc` provides the Linux boundary. The build script invokes a C++
compiler only to produce independent original queue, VisionIPC and ABI peers;
it does not archive or link those implementations into Rust consumers. The
existing public methods and metadata fields remain. The unused
`Error::Native(cxx::Exception)` variant is replaced by typed Rust errors after
checking workspace consumers for matches/construction of that variant.

The image implementation uses relaxed 64-bit atomics for complete pixel words.
Partial writes update those same words with a masked compare-and-swap; only the
disjoint final pixel remainder uses byte atomics. The frame-ID trailer remains
separate, including the original unaligned imported-buffer case. This preserves
the original non-atomic whole-frame copy contract without constructing ordinary
Rust references to bytes that another participant can update. Fresh Miri,
sanitizer and target checks cover this implementation; no speedup is claimed
from this implementation choice.

On the supported Linux little-endian 64-bit targets, the original VisionBuf
handshake record is 112 bytes and a frame notification is 48 bytes. These layouts
must be checked against compiled original peers, including the unaligned
frame-ID trailer regression. Imported process-local pointers are never trusted.

## Baseline evidence and test correction

The original CXX baseline initially failed the retained VisionIPC FD test before
any production edits. That test read a 96-byte payload through one duplicated
FD and later tried to read the payload again through another duplicate without
resetting the shared file offset. The original executable's syscall trace showed
an eight-byte frame-ID trailer followed by EOF on that later read.

The test now uses `FileExt::read_exact_at(..., 0)` for the retained payload check.
The descriptor sharing contract is unchanged. The corrected CXX queue and
VisionIPC suite passes, and its executables, source hashes, original failure,
syscall trace and exact test diff are retained separately from new Rust evidence
under `.omo/evidence/native-ipc-194/`. This test correction is also being carried
into the parent encoder integration in #190/#195.

## Component verification

The final generic suite passes 46 tests on x86-64, 46 tests as actual aarch64
executables under QEMU, and 46 tests under Rust AddressSanitizer with original
C++ peers instrumented by AddressSanitizer and UndefinedBehaviorSanitizer. These
totals count the top-level 30 unit and 16 integration tests; spawned child cases
are not counted again. Coverage includes all three process directions, four
VisionIPC streams, queue wrap/overwrite/conflation, forty-reader eviction,
publisher replacement, twelve-process queue creation, eight-process publisher
contention, malformed queue records and descriptor handshakes, partial-import
cleanup, idle-client listener shutdown and retained-buffer restart behavior.

A real signal test interrupts a 120 ms receive ten times. Its retained syscall
trace has eleven nanosleep requests with strictly decreasing remaining times,
ten SIGUSR2 deliveries and a timeout without a packet. Infinite polling is
separately exercised through successive 100 ms waits. Namespace, lazy-batch,
queued-batch and transient-publisher tests retain their existing public APIs.

The separate ION checker executes original C++ and Rust public-API probes against
a real-FD/mmap driver fixture. All 40 normal and fault cases pass on x86-64,
aarch64/QEMU and instrumented x86-64. It compares allocation size/alignment/heap,
descriptor sharing/import, mapped length, cache command/range, unmap/free order,
the 101-call interrupted-ioctl limit and error continuation. Poisoned backing
storage verifies allocation clearing; writing before import verifies preservation
of existing image bytes. A driver-rejected free is recorded as a retained handle,
not reported as successful driver cleanup. Original fail-stop allocation/import
errors and Rust returned errors are recorded separately, including Rust cleanup.
Six positive-nonzero ioctl cases fail against the earlier frozen Rust probe and
pass after matching the original retry condition: only `-1` with `EINTR` retries.
The target cache length retains the original `size_t` to `unsigned int` narrowing.
ION-feature unit/ABI tests pass 27/27 on both host and aarch64.

The final pure-memory suite passes 21 tests in five Miri configurations: host
default; host and aarch64 strict provenance with symbolic alignment and 0.1
preemption; and host and aarch64 Tree Borrows. This includes partial-word
compare-and-swap updates from two threads and disjoint unaligned frame-ID bytes.
Miri does not execute Linux mappings, socket syscalls or the ION driver. ASan
instruments the project Rust and C++ code, not the complete standard library or
kernel. Earlier CXX, byte-access and sanitizer receipts remain separate historical
evidence; they are not substituted for these final results.

Focused all-target/all-feature Clippy with `-D warnings`, Rust formatting and the
Python checker's Ruff validation pass. No dependencies were installed for these
checks. Builds used two jobs, disabled incremental compilation and checked the
25 GiB free-space floor plus a bounded growth allowance before each operation.

The release aarch64 ION caller's linker map contains the Rust transport objects
and no original transport/CXX objects. Symbol and dependency audits of eighteen
host/ARM Rust test executables plus that release caller find no original msgq,
VisionIPC or CXX-adapter runtime symbols and no `libstdc++` dependency. Original
C++ peer executables are deliberately excluded from that assertion.

The original AGNOS 19.8-carrot-bt1 extracted loader resolves the release caller
using only `libc.so.6` and `libgcc_s.so.1`. With those same extracted libraries,
the generic VisionIPC server/client/image lifetime test and 25 selected
memory/queue/wire/socket unit tests execute successfully under QEMU. The ION
caller reaches and reports absent `/dev/ion`; no physical driver was provided.
The separate 40-case ARM driver fixture uses the GNU cross-toolchain userspace,
not the AGNOS userspace. These are distinct verification claims.

Final local evidence is retained beneath `.omo/evidence/native-ipc-194/`:

| Evidence | Result | Receipt/report SHA-256 |
| --- | --- | --- |
| `final-host/receipt.json` | 46 generic tests; frozen executable/source identities | `40403911e87bf6e17cb331323d458d665c3ff1e66fc3b0129d9c4e45656b9a62` |
| `final-arm/generic-receipt.json` | 46 generic aarch64 tests; executions in `runs.json` | `e3c4b0c8da429e5d7393f513869b21027c24f6763c3bbefff6e4f3398cce7f81` |
| `final-ion/contract/report.json` | 40 host ION fixture comparisons | `2c0d93e84112f338e82af3f66e9f0d6633ca71a95903069394b234d70aedbbc8` |
| `final-arm-ion/contract/report.json` | 40 ARM ION fixture comparisons | `28939cda163a523490b6575144b0c3abd5c306fbe9555e4873e3efee0df1cfc1` |
| `final-asan/contract/report.json` | 40 ION sanitizer comparisons; generic suite in `test-stdout.log` | `0c905320b8374562862538a71f04ce706bf28d45e5afaeb91a313e9605064b8d` |
| `final-linkage/receipt.json` | 19 Rust artifacts, release map and AGNOS library identities | `186038aaf5f3a64cd444d56bd3a88e617aa3ec45f693ac6a3aeef4457fe1dc50` |

The final host/generic ARM receipts precede the one-line ION-only cache-length
cast correction; final ION, ASan and release-map artifacts include it. Their
input hashes preserve that distinction. `final-memory/` contains each Miri
command and complete log. Raw binaries and local evidence are not Git content.

The checker is `rust/tools/check_msgq_ion.py`; its original caller and fixture
are `rust/crates/msgq/native/ion_contract.cc` and
`rust/tools/msgq_ion_fixture.cc`. The Rust caller is the `ion_contract` example
with `visionipc-ion`. Generic process tests support explicit
`IPC194_TEST_QEMU`/`IPC194_TEST_SYSROOT` configuration without installing binfmt
handlers or changing system process execution.

Useful component commands from `rust/` are:

```sh
cargo test -p openpilot-msgq --locked -- --test-threads=1
cargo test -p openpilot-msgq --features visionipc-ion --lib --locked -- --test-threads=1
cargo clippy -p openpilot-msgq --all-targets --all-features --locked -- -D warnings
cargo +nightly-2026-09-29 miri test -p openpilot-msgq --no-default-features --lib --locked
cargo build -p openpilot-msgq --features visionipc-ion --example ion_contract --locked
python3 tools/check_msgq_ion.py --source ORIGINAL_ION_CALLER --native RUST_ION_CALLER --fixture ION_FIXTURE_SO --output EVIDENCE_DIR
```

Build the original ION caller against `msgq_repo/msgq/{ipc,event,impl_msgq,impl_fake,msgq}.cc`
and `visionipc/{visionipc,visionipc_client,visionipc_server,visionbuf_ion}.cc`,
including `msgq_repo` and `third_party/linux/include`, with C++17, pthread and
assertions enabled. Build the fixture as a shared PIC library with the same
kernel-header include and `-ldl`. Exact compiler arguments and executable hashes
are preserved in each final ION evidence directory. Target runs additionally
supply the checker's `--qemu` and `--sysroot` arguments.

## Isolated dev integration, 2026-10-07

The integration reuses the final native transport from `1dd955e1`, including
the musl socket-length correction and #211 notification wait. The original
transport is retained only as an executable comparison peer. The current
Card/planner candidate uses this Rust transport without changing their policy.

The package's complete generic tests pass, including real C++ peers, queue
races, signal masks, restart and VisionIPC ownership. One existing planner IPC
comparison passes all four source/native runs with 121 publications each and
no content differences. One existing Nissan Card IPC profile passes 320 warmup
and 80 measured steps, including full CAN/publication fields and Params drain.
The executed planner SHA-256 is
`dfba1df6c904fdb59b4bde8e86143c44f8bf913fbc28281649aa0463d90be313`;
Card is `f47c17e80cb49eb0fefdf17a462c8a7adf2baaee4470d95be22e49cfead50baf`.
Local receipts and final executables are retained in
`.analysis/archive/2026-10-07-native-ipc/`. CI policy checks pass 23 tests and
180 subtests. Existing independent ARM/ION/sanitizer receipts above are reused;
they were not repeated locally. The existing memory job now also runs the native
IPC pure-memory cases with ARM strict provenance and Tree Borrows. Exact-head
hosted gates and dev integration remain pending for this candidate.

## Remaining external boundary

Linux shared mappings, file locking, signals, Unix sockets and SCM_RIGHTS remain
OS dependencies. The target allocation mode additionally depends on the AGNOS
ION driver and its cache-maintenance ioctls. Host emulation and driver fixtures
do not establish physical ION, camera or vehicle operation.

Normal startup, existing log upload, exact-head Actions, post-merge checks and
the user's first device comparison remain separate completion gates. No CPU
savings or device acceptance is claimed by this document.
