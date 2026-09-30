# Continuous log-space deleter

Issue [#42](https://github.com/bin9208/openpilot-rust/issues/42), under whole-runtime
[#1](https://github.com/bin9208/openpilot-rust/issues/1). Source provenance is
`openpilot/system/loggerd/{deleter,uploader,xattr_cache,config}.py` and
`openpilot/system/hardware/hw.py` at the pinned repository source.

`openpilot-deleter` keeps the original strict 5 GiB/30 percent free-space gates,
two separate statvfs reads and safe defaults after stat errors. The directory
sort preserves the 2024-format preference, last-segment numeric padding and
Python filesystem Unicode/surrogateescape ordering. Marked non-segment names
still count against the original five-marked-entry window. Each valid marked
segment expands to itself and its two predecessors. Integer suffix handling
preserves signs, underscores, Unicode decimal digits and arbitrary-length values
within Python's decimal input limit; the decimal table matches Unicode 15.0.

Preserve xattrs are cached for the lifetime of the daemon, including absent
attributes, matching the original cache. Preserved segments are deferred, not
immune from deletion after other candidates are exhausted. Boot/crash have the
original later priority. Direct children ending in `.lock` suppress a candidate.
Removal errors continue to the next directory; preserve-xattr errors escape the
cycle. Top-level directory symlinks remain untouched, matching shutil.rmtree.

Low-space cycles delete at most one directory and wait 100 ms; normal cycles wait
30 seconds. SIGINT/SIGTERM interrupt waits through 20 ms checks. `--cycles N`
provides a positive bounded host-QA run, without changing the production default.
`LOG_ROOT`, an empty override, HOME/prefix and /TICI select source paths. No
production manager entry changes here.

## Host observations

- Three filesystem regressions cover strict thresholds, original preservation
  priority, cached attributes, lock skipping and symlink target isolation.
- Nine subprocess path cases cover unset/named/empty prefixes and unset/empty/
  explicit LOG_ROOT, using temporary HOME roots only.
- The source oracle executes the actual original ordering functions, xattr cache
  and complete deleter loop against a twin temporary filesystem: 27 scenarios,
  76 comparisons with exact selected directories, preserved sets, wait values,
  failures and remaining filesystem entries.
- Native continuous QA uses a private 16 MiB tmpfs mounted only inside a separate
  namespace. Actual statvfs reports less than 5 GiB without filling a user
  filesystem. Both source and Rust delete the same three directories, retaining
  the locked segment and boot directory. Observed Rust intervals were 100.239
  and 100.200 ms. Idle SIGINT/SIGTERM exits were about 10.4 ms; invalid CLI values
  return nonzero. Timing values are host observations, not device guarantees.

All fixtures are newly created synthetic directories; existing route logs and
user captures are not used. Namespace teardown unmounts the temporary filesystem.
Local native QA uses an unprivileged user/mount namespace. CI uses passwordless
sudo only to create an isolated mount namespace, accommodating runner user-
namespace restrictions; all deletion paths still reside inside its private tmpfs.

## Reproduction

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-deleter --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-deleter --bins --examples --locked
python rust/tools/check_deleter_reference.py \
  --binary rust/target/debug/examples/deleter_trace --output /path/to/fresh-reference
python rust/tools/check_deleter_daemon.py \
  --binary rust/target/debug/openpilot-deleter --output /path/to/fresh-native
```

Rust CI repeats the comparisons and includes the daemon in generic GNU/musl
aarch64 workspace builds. Local evidence is indexed in the private
`.analysis/archive/2026-09-30-rust-deleter/` record. Cloud and post-merge
results are recorded on #42 after they finish.

Whole-runtime startup, remaining services, logging/upload integration and the
first user device comparison remain open. This change establishes no AGNOS,
vehicle or CPU-saving result.

Docs-Not-Needed: internal daemon port preserving existing production settings.
