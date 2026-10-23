# Encoder reconnection — issue #191

Parent audit: #497. Stable base
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, branch
`codex/fix-497-stale-runtime-defects`. Source and fixtures remain uncommitted for
parent review. This record covers only `encoderd/src/native/runtime.rs` and its
exclusive `check_encoder_runtime.py` oracle. No msgq, codec or scheduling policy
is changed. Hosted runtime results and exact-SHA Actions links are pending.

## Runtime contract

The native inner loop previously ignored a changed VisionIPC server even after
`receive_retained` cleared the connected flag. A true disconnection now leaves
that loop, releases cached import mappings, and reconnects through the existing
outer loop. Ordinary receive timeouts keep the connection. Codec-owned Arc
mappings remain pinned through asynchronous use; matching-layout reconnects keep
codecs, publishers, synchronization and lifecycle/segment counters.

A changed **complete** `VisionLayout` (width, height, stride, UV offset or
allocation length) is rejected immediately after importing the replacement
server, before receiving any input for an existing codec. It reports
`encoder stream layout changed after reconnect; supervisor restart required`
and exits through the existing explicit fatal/SIGABRT path. A new owned process
must initialize codecs for that layout. This does not implement in-process
geometry reconfiguration, reset codec indices on a same-layout reconnect, or
change source rotation behavior.

## Repaired restart oracle

The prior candidate checker compared only server-A packets and excluded every
server-B packet, while incorrectly marking the entire result exact. The new
checker still preserves and tests the unchanged original encoder's receive-only
restart failure, but adds an **uninterrupted original encoder process** as an
independent byte/metadata oracle for the repaired native output.

The restart fixture has a fixed A history, frames **370..499**. Both original,
native and control receive identical pixels/layout/timestamps and must produce
identical complete A publications. This keeps B frames 500..519 within the
already warm source segment 450..529. The former fixture's 229->500 frame jump
forced multiple source-policy rotations, introducing codec startup buffering
that would conflate missing transport inputs with the separate inherited codec
policy. The fixed replacement history preserves a real pre-boundary rotation
and leaves all other nine scenarios' histories/comparisons unchanged. No history
or bound is selected from observed native packet loss.

Generation B uses the following bounded handshake:

1. Send frame500. Original `msgq_init_publisher` invalidates old reader slots.
   A read-only snapshot of the actual `msgq_header_t` must show that the encoder
   re-registered and caught up before sending frame501. The checker never writes
   queue registration or pointers.
2. Frame501 can be the new-server-ID rejection packet, or the first accepted
   frame if frame500 already triggered import replacement. Only the initial
   prefix of at most **two inputs**, 500/501, may be lost. No additional probes
   are sent while waiting for recovery.
3. Before sending 502..519, the checker observes replacement buffer imports in
   `/proc/<pid>/maps`: all64 new camera allocations present and no old camera
   allocation still mapped. Failure to reconnect times out instead of enlarging
   the permitted loss window.
4. Each video service must publish a contiguous sequence starting at501 or502
   through519, with the **same start for both codecs**, unchanged geometry,
   increasing encodeId and valid segment/segmentId continuity. Later holes,
   duplication, arbitrary missing prefixes and per-codec divergence fail.
5. Only after those independent assertions succeed does the uninterrupted
   original control receive the fixed A history plus the complete range from
   that bounded first accepted input through519. It never receives an arbitrary
   list/intersection filtered to native output. Input layout and SOF/EOF metadata
   must match exactly. All normalized publication fields and **actual encoded
   header/payload bytes**, not merely their hashes, must equal the candidate's
   complete A+B output.

The result records `original_restart_failure_preserved`, a separate boolean
`repaired_restart`, `post_restart_bytes_exact`, the comparison scope, and a
`repaired-restart.json` control receipt. Exactness is reported only after the
post-boundary comparison succeeds. The original failed-restart process remains
separate evidence, not a waived successful post-boundary oracle.

## Actual layout rejection and supervisor replacement cases

Four owned real-VisionIPC cases run inside `main-restart`, so the existing
`check_encoder_ci.py` runtime invocation still returns its expected ten scenario
results and automatically executes this additional acceptance work:

- Change image dimensions and valid padded NV12 storage.
- Preserve dimensions but change stride and its required storage/UV offset.
- Preserve dimensions/stride but move UV offset and extend storage.
- Change allocation length alone, preserving dimensions, stride and UV offset.

Each starts a native encoder on A, sends100..169, and waits for normal output.
After replacing the server with B it requires SIGABRT **and the exact layout
contract diagnostic**, rejects sanitizer failures, and checks that no B
publication reached the old codec. A dimensions-only comparison cannot pass the
padding-only cases. The process must terminate without the checker supplying
SIGTERM; a hang, SIGSEGV or unrelated codec error is not accepted as rejection.
Expected aborts retain their actual return code/diagnostic while core dumps are
disabled in owned test children.

The checker then launches a distinct native child against the **same live server
B, IPC namespace and Params**. It verifies imported mappings and executed-ELF
identity for that new PID, sends the complete fixed100..169 history, checks video
geometry, fresh indices/segment0 and contiguous lossless output, and compares all output
bytes and fields against another original encoder that starts normally on the
same new layout in its own namespace. Fresh H264 startup buffering remains
source-owned: both new processes receive all70 inputs, and no control inputs are
chosen from native outputs. Lossless road output must contain every frame;
H264 startup/output must exactly equal the independent original process.
Thumbnail bytes are also compared. Old failure captures, replacement captures,
PID/exit receipts, layouts and source control captures are retained separately.

## Hosted-only verification and baseline RED

No functional tests, processes, VisionIPC fixtures, builds, performance trials
or device operations run locally. Local validation is limited to Python AST
syntax, Rust formatting/parsing and scoped diff inspection. The new checker was
written before updating the production fatal diagnostic. The parent owns
exact-SHA hosted RED/GREEN, aggregate CI, commit/push and acceptance updates.

The existing encoder-runtime PR check already builds the native/original
artifacts and calls `check_encoder_ci.py`; no shared CI edit is needed. With its
existing msgq Python/Cap'n Proto/FFmpeg/libyuv/libjpeg setup, the complete hosted
entrypoint remains:

```sh
python3 rust/tools/check_encoder_ci.py \
  --binaries "$CARGO_TARGET_DIR/debug" \
  --build-messages "$RUNNER_TEMP/encoder-build.jsonl" \
  --output "$RUNNER_TEMP/encoder-ci"
```

Use the job's existing native/capnp/json11 arguments when configured. Its
`runtime` child executes all ten existing scenario names and now includes the
uninterrupted restart control and all four layout rejection/replacement cases.
For a focused hosted rerun after that job has staged the original binary:

```sh
PYTHONPATH="$ENCODER_MSGQ_PYTHON:$GITHUB_WORKSPACE:$GITHUB_WORKSPACE/rust/tools" \
  python3 rust/tools/check_encoder_runtime.py \
  --source "$RUNNER_TEMP/encoder-ci/source/original-encoderd" \
  --rust "$CARGO_TARGET_DIR/debug/openpilot-encoderd" \
  --native / \
  --output "$RUNNER_TEMP/encoder-restart-191" \
  --case main-restart
```

Substitute the existing job's `--native` root if it uses staged native libraries.
Also retain the existing checker lifecycle/dependency tests in hosted Actions:

```sh
PYTHONPATH="$GITHUB_WORKSPACE/rust/tools:$PYTHONPATH" \
  python3 -m pytest -q rust/tools/tests/test_encoder_outcomes.py rust/tools/tests/test_encoder_ci.py
```

Require ordinary package checks and existing affected ARM builds separately;
host PC codec equality does not establish physical VIDC/device behavior.

For actual baseline RED, keep the candidate checker and support fixtures in an
independent disposable GitHub-hosted checkout, restore **only** production
`runtime.rs` from the stable base, and rebuild there:

```sh
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/encoderd/src/native/runtime.rs > rust/crates/encoderd/src/native/runtime.rs
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-encoderd --features native --bins --examples
```

Then run the focused command above with that baseline binary. Expected RED is a
runtime failure to replace old mappings/resume B, not a compilation or dependency
error. Candidate GREEN must use a separate unchanged candidate checkout and
require both the focused acceptance scenario and complete encoder CI. None of
these substitutions or executions has been performed in the shared worktree.

Remaining: actual hosted receipts/Actions URLs, affected target build results,
and physical VIDC/device acceptance. No resized-device or performance result is
claimed from this source patch.
