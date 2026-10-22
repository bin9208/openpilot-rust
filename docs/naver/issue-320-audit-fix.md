# #320: bounded QCOM diagnostic stream retention

Scope: [issue #320](https://github.com/bin9208/openpilot-rust/issues/320), assigned
under #497. Base `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, branch
`codex/fix-497-stale-runtime-defects`. Changes are uncommitted for the parent to
review. Hosted RED/GREEN and integration results are pending; no device access,
fixture execution, local test/build or performance claim.

## Failure mechanism and source contracts

`Frames::extend` copied every serial transfer into `pending`; `next_frame` only
released bytes after a raw 0x7e. Repeated delimiter-free transfers caused unlimited
retention and repeated rescanning, inherited from original `ModemDiag.recv`.
The 0x10000 buffer in `serial::recv` is a transfer size, not a frame maximum.

There is no universal length field in this HDLC envelope. Quectel's
[QLog transport description](https://github.com/quectel-official/QLog/blob/main/mdm.c)
also describes variable information plus a 16-bit CRC and ending flag; its
separate non-HDLC envelope's u16 length cannot bound this protocol. Source
`decode::Log::parse` requires its u16 inner/outer extents to equal payload length
minus three. A complete accepted DIAG_LOG therefore needs at most 65,538 payload
bytes, plus one opcode: 65,539 decoded message bytes. Including CRC and full
escaping would require at most 131,083 wire bytes, but that bound is log-specific.

All setup callers were read: config replies need at most the first 75 payload
bytes (3 reserved + operation/status + sixteen u32 counts), mask replies need 11;
NV/subsystem reply bodies are unused. Setup skips DIAG_LOG before extent
validation, including malformed or arbitrarily long ones. Steady-state handling
uses complete logs and only the opcode of other replies. No universal wire-frame
cap is introduced.

## Implementation and intentional API differences

The parser unescapes incrementally, retains at most 65,539 decoded message bytes,
and holds the last two decoded bytes separately for CRC. It continues CRC over
discarded tails; a delimiter-free stream causes no length counter overflow and
no allocation proportional to total input. Noncanonical/dangling escaping follows
the previous Rust decoder. CRC failure precedes body/extent/overflow rejection.
It waits for the delimiter before returning an error, then starts a fresh frame;
following coalesced frames and partial tails remain ordered.

Completed messages use a queue. Production drains that queue before reading its
next 65,536-byte transfer, bounding queued decoded data by one transfer plus the
retained prefix of the previous unfinished frame. Callers of the low-level
`Frames::extend` API must drain completed messages; accumulating unlimited
completed frames without consuming them is outside this streaming contract.
The unfinished prefix Vec can have allocator capacity up to twice its byte limit;
it never reserves from a wire length. Work is linear in input bytes.

Mode-independent retention avoids applying the wrong policy to a frame already
buffered when switching setup/runtime/teardown. `Diagnostic::exchange_prefix`
explicitly returns a validated non-log prefix up to 75 bytes; `recv_log` returns
complete logs and opcode-only non-log replies. Only the production call sites in
`setup.rs` and `daemon.rs` select those APIs. These small boundary changes are
necessary to avoid silently truncating the old full-payload API.

`Frames::next_frame`, `Diagnostic::recv` and the existing full `exchange` never
silently truncate: they now explicitly report `diagnostic frame exceeds retained
payload` for a CRC-valid message over 65,539 decoded bytes. Those full APIs are
not used by production setup/runtime. Runtime oversize logs retain the existing
`diagnostic log extent` error; arbitrary setup replies, skipped logs and unknown
runtime opcodes remain accepted after full CRC validation. The existing
0..4096 range-count admission, selected logs, scheduling, protocol limits and
publication comparisons are unchanged. No oracle waiver was added.

## Tests written before the implementation and hosted commands

The initial retention test and framing regressions were written before production
changes. Per the explicit task instruction, execution is deferred to the parent's
GitHub-hosted Actions. The retention test uses the real parser on 128 successive
4 KiB transfers and inspects its private retained Vec without a runtime testing
API. It requires <=65,539 retained bytes and bounded capacity after every read.
It can run on the original production parser without new runtime APIs.

For an actual baseline RED, use an **independent disposable hosted checkout** at
the base SHA. Copy only candidate
`rust/crates/qcomgpsd/tests/data/framing_retention.rs` there and append this
**test-only** hook to its original `src/framing.rs`:

```rust
#[cfg(test)]
#[path = "../tests/data/framing_retention.rs"]
mod retention_tests;
```

Run there (expected nonzero assertion failure for retained byte growth; do not
accept compilation/setup failures as RED):

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd --lib framing::retention_tests::delimiter_free_retention_is_bounded -- --exact
```

On the candidate, run in GitHub-hosted Actions only:

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd
cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd --all-targets -- -D warnings
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-qcomgpsd --bins --examples
python3 rust/tools/check_qcomgps_framing.py rust/target/debug/examples/framing_trace .analysis/scratch/issue-320/framing-source.json
PYTHONPATH=.:rust/tools python3 rust/tools/check_qcomgps_reference.py rust/target/debug/examples/qcom_trace .analysis/scratch/issue-320/publications.json
PYTHONPATH=.:rust/tools python3 rust/tools/check_qcomgps_daemon.py rust/target/debug/openpilot-qcomgpsd rust/target/debug/examples/qcom_subscribe .analysis/scratch/issue-320/daemon
```

Binary paths above assume the standard `rust/target`; substitute the parent's
hosted `CARGO_TARGET_DIR` if configured. Use the parent's existing hosted Python,
Cap'n Proto and msgq dependencies/environment. The pre-existing daemon checker
must run as an ordinary subprocess (not a shell exec replacing its session
leader), as already documented in the prior #320 record. No shared CI file is
changed by this patch.

Added cases cover maximum valid logs with heavy escaping and split escape pairs;
explicit full-API overflow; CRC priority on oversized data; error/delimiter
recovery; setup/runtime/setup transitions; valid 150,000-byte non-log tails and
skipped logs; corruption beyond the retention boundary; and interruption of an
unterminated stream through the real serial stop path. Existing mask-count tests
continue exercising complete admission before any mask commands.

The new original/native framing checker executes the unchanged original
`hdlc_encapsulate`/`hdlc_decapsulate` AST bodies, using an independent stdlib CRC
implementation already used by the diagnostic peer. It compares all 256 opcodes,
log-sized boundary frames and corruption followed by recovery, with a source
SHA256 in its output. It adds no exemptions to the existing publication or
setup/teardown transcript comparisons. Both the new runtime fixture example and
all fixtures run only in hosted Actions.

## Remaining acceptance

Local Rust formatting/parsing, Python AST syntax and scoped diff review are the
only checks permitted here. The parent must obtain actual baseline RED and
candidate GREEN, unchanged original/native publication and setup/teardown
comparisons, and relevant hosted build evidence before acceptance. Record the
candidate SHA and Actions links in the parent issue/PR. Physical modem/device
behavior remains unverified; no target runtime or resource/performance claim is
made from this source patch.
