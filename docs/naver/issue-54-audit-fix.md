# Issue #54: raw-video I/O failure completion

Tracking: [#54](https://github.com/bin9208/openpilot-rust/issues/54), parent
[#497](https://github.com/bin9208/openpilot-rust/issues/497).
Baseline: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
Worktree: `backlog-fixes-497/openpilot-rust`, branch
`codex/fix-497-stale-runtime-defects`. Status: scoped patch prepared; hosted
RED/GREEN, parent review and exact-SHA acceptance remain pending. No commit,
push, GitHub mutation, device operation or local functional test was performed.

## Failure and bounded correction

The inherited C++ `safe_fwrite` retries until its byte count reaches the
request, but does not check `ferror`. A buffered stream can report a complete
count with its stream error set after ENOSPC. Its raw-video destructor ignores
both `safe_fflush` and `fclose` results and always unlinks the completion lock.
The Rust wrapper/writer retained those behaviors, including reporting a raw
write failure without retaining that failure for completion.

The stdio boundary now checks short counts and stream errors after `fwrite` for
raw video. It never clears a raw-video error or treats EINTR as recovered:
buffered bytes may already be lost, even when fwrite returns the complete count.
Raw-video finish captures a flush failure (including EINTR) before closing,
always closes the owned FILE exactly once, checks close failure, and returns the
first finish error. It adds no fsync, buffer-size change or daemon/encoder buffer
redesign.

`VideoWriter` remembers the first raw write error. Close still executes actual
flush/close even after write failure, reports an Error-level
`raw video incomplete; marked ... errno=...` diagnostic, and atomically renames
the existing `<video>.lock` to `<video>.failed` after FILE close. Reusing the lock
inode avoids allocating a new failure-marker inode on ENOSPC. The marker means
**closed and failed**, never an active writer: uploader scanning excludes the
damaged file and marker, while healthy files stay eligible and the segment
remains reclaimable by the unchanged deleter. Uploader startup lock cleanup
preserves failure markers. Active `.lock` behavior is unchanged. Marker rename
errors remain checked, like existing lock-removal metadata errors; successful
failure marking never claims that damaged data became valid.

Per-video failures remain nonfatal: returning an error into the
existing `Encoder::handle` rotation path would abandon the next segment's open
rlog/qlog, and returning one during shutdown would skip later video writers.
The existing daemon already finalizes rlog/qlog before closing encoders. Normal
raw bytes, index order, route sentinels and healthy-file completion remain
unchanged. The only uploader policy addition is per-file failure-marker
exclusion; its `lib.rs` and the deleter implementation are untouched. The separate
`RawFile::write` and `RawFile::finish_checked` bootlog policies are unchanged;
only the raw-video caller selects checked stream errors.

The focused oracle exception applies only to controlled raw-video faults:
added exact Error-level errno=28 (ENOSPC) or errno=4 (EINTR) write/incomplete
diagnostics and the per-file `fcamera.hevc.failed` marker. Exit remains zero
after normal SIGTERM. The oracle
still compares every normalized rlog/qlog record, all index fields, all other
file names, all unaffected diagnostics in order and verified diagnostic context.
Existing MPEG-TS packet/trailer/AVIO-close checks and exact source comparisons
are unchanged. It does not weaken any numeric/timing/output-equivalence limit.

## Regression cases (written before implementation)

- Production `VideoWriter` with a symlink to `/dev/full`: an 8 KiB packet forces
  ENOSPC during write; a short packet delays ENOSPC until buffered flush. Both
  must have a literal `fcamera.hevc.failed` file and no `fcamera.hevc.lock`.
  The existing successful-byte test still requires exact header/payload bytes,
  lock removal and absence of a failure marker.
- Native original and candidate processes, driven through real msgq, receive
  three indexed road frames. Real strace captures must show `/dev/full` ENOSPC
  and a single descriptor close. In the small-packet case there must be no
  ENOSPC before SIGTERM. Both rlog and qlog must contain indices 0,1,2 and the
  final SIGTERM end-of-route sentinel, and `rlog.lock` must be removed.
- A host-only fclose interposer releases the real raw road FILE once, then
  returns ENOSPC. Its stderr marker must appear once; exact video bytes and
  complete rlog/qlog are checked. The production binary contains no injection
  hook. Only the exact raw-road filename is intercepted; all other I/O is real.
- Every native case also records healthy wide-camera bytes. Its writer follows
  road in the daemon's shutdown loop; its exact bytes and removed lock prove
  that handling the road failure does not skip subsequent writers.
- A real libc fopencookie stream loses its buffered header when its write
  callback returns EINTR during fflush. The helper then calls the actual libc
  fwrite for the next payload, requires its full returned count and real sticky
  ferror, and returns that actual count with errno=EINTR. It does not mock a
  successful fwrite or manufacture a FILE error flag. The fixture requires the
  missing header as byte-level evidence, persistent failure marking despite
  later full payload writes, and complete healthy route logs/videos. This
  catches the reviewed clearerr-and-advance bug.
- Every candidate native run invokes the actual `uploader_trace` scanner on its
  recorded segment. Healthy qlog/rlog/wide-video files must remain eligible,
  while the failed road video and marker must be absent. The actual unchanged
  `deleter_trace` then removes that same segment under simulated low-space
  pressure; decoded comparison captures remain outside it for source review.
- Focused uploader/deleter regression additionally covers marked priority video
  (`qcamera.ts`) and raw video, metered/unmetered scans, retained active-lock
  exclusion, startup cleanup preserving failure evidence, and reclamation.

The first reviewed lock-retention patch incorrectly blocked every file in a
failed segment from upload and prevented reclamation. The revised tests were
updated before this correction; the old literal lock assertions were removed.

## GitHub-hosted Actions commands only

These commands belong in the parent's GitHub-hosted aggregate job, using the
existing native logger dependencies (FFmpeg development libraries, capnproto,
libyuv, strace, C compiler and oracle Python dependencies). They have **not**
been run locally. From repository root:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-loggerd --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-uploader --test video_failures --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-deleter --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-loggerd --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-uploader --example uploader_trace --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-deleter --example deleter_trace --locked
python3 rust/tools/build_msgq_python.py --output "$RUNNER_TEMP/logger-msgq"
python3 rust/tools/loggerd_native_build.py --output "$RUNNER_TEMP/logger-original"
PYTHONPATH="$RUNNER_TEMP/logger-msgq:$GITHUB_WORKSPACE:$GITHUB_WORKSPACE/rust/tools" \
  python3 rust/tools/check_loggerd_io.py \
  --binary rust/target/debug/openpilot-loggerd \
  --original "$RUNNER_TEMP/logger-original/original-loggerd" \
  --uploader rust/target/debug/examples/uploader_trace \
  --deleter rust/target/debug/examples/deleter_trace \
  --output "$RUNNER_TEMP/logger-io-54"
```

When the existing full native/media check has generated its synthetic encoder
fixtures, run the existing combined command as well (no equivalence exemption
for remux output):

```sh
PYTHONPATH="$RUNNER_TEMP/logger-msgq:$GITHUB_WORKSPACE:$GITHUB_WORKSPACE/rust/tools" \
  python3 rust/tools/check_loggerd_io.py \
  --binary rust/target/debug/openpilot-loggerd \
  --original "$RUNNER_TEMP/logger-original/original-loggerd" \
  --uploader rust/target/debug/examples/uploader_trace \
  --deleter rust/target/debug/examples/deleter_trace \
  --fixtures "$RUNNER_TEMP/logger-native/fixtures" \
  --output "$RUNNER_TEMP/logger-io-54-with-remux"
```

For actual baseline RED, keep the candidate tests/oracle and replace **only** the
two production files with baseline content inside a GitHub-hosted job. This
preserves the original runtime API used by the integration tests; expected RED
is two failed literal failure-marker assertions, not a compilation error:

```sh
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/loggerd/src/raw_file.rs > rust/crates/loggerd/src/raw_file.rs
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/crates/loggerd/src/media.rs > rust/crates/loggerd/src/media.rs
cargo test --manifest-path rust/Cargo.toml -p openpilot-loggerd --locked \
  --test media marks_raw_video_failed_after_enospc -- --nocapture
```

The parent job must explicitly expect that failure, retain its output, restore
the two production files from its candidate SHA, then require GREEN with the
same command and the full package/native checks above. No baseline substitution
has been performed in the shared worker worktree.

For the uploader baseline RED, retain the candidate `video_failures.rs` test,
restore only `uploader/src/scan.rs` from the baseline in the disposable hosted
checkout and run the focused uploader command above. Expected RED is that the
scanner includes damaged files/markers in its returned candidates. Restore the
candidate scanner before GREEN. For isolated P2 RED, retain candidate media,
scanner and oracle, and apply the small retained
`loggerd/tests/raw_eintr_review_red.patch` inside the disposable hosted checkout.
It restores the reviewed clearerr-and-advance behavior on EINTR without undoing
the P1 marker/scanner correction. Rebuild loggerd and run the native I/O checker
with `--raw-phase eintr`; expected RED is that its real lost-header/full-count
FILE is accepted as completion and no failure marker is produced. Restore
`raw_file.rs` from the candidate before rebuilding and requiring GREEN. The
patch has not been applied or executed locally. Parent owns recording the exact
hosted source SHA/output for the RED and GREEN runs.

The normal raw I/O checker now requires `--uploader` and `--deleter`; the parent
must pass the two built probe paths in its shared CI command. `--remux-only`
remains unchanged and does not require the probes.

Local validation remains restricted to Rust formatting, Python AST syntax, C
`-fsyntax-only -Wall -Wextra -Werror`, and scoped diff review. Hosted
runtime outcomes and Actions URLs are not yet available. Parent owns aggregate
CI, source review, commit and acceptance updates; leave #54 open pending those
results. This host fault-injection fix makes no device-frequency or driving
claim.

Docs-Not-Needed: internal recording failure completion; no user settings,
production selector or paired user documentation changed.
