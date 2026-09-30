# Native checkout identity and update status

Scope: [#92](https://github.com/bin9208/openpilot-rust/issues/92), under the
approved full-runtime conversion. Source:
`openpilot/system/manager/update_status.py`. Implementation:
`rust/crates/checkout-status` and the captured-exec extension to the existing
`process-supervision` helper. Production manager selection is unchanged.

## API and state

`read_checkout_commit(repo, launcher)` returns an optional lowercase commit.
`UpdateStatus::new(repo, launcher)` captures that identity once; its
`running_commit()` accessor never changes. `update(now)` accepts monotonic
seconds and returns the current reboot-required flag. Callers construct it
under the existing startup checkout lock; this library does not acquire or
replace that lock.

The first check is due at zero. Values strictly below the next deadline retain
the existing result without reading the checkout. A due check sets the next
deadline to `now + 5.0` before reading. Two successful, matching identities
different from the startup identity are required. Failed reads, the startup
identity, or a different pending identity reset the appropriate state. A
missing startup identity is never recaptured. Native `f64` comparisons and
addition retain the source behavior at exact boundaries, backward times,
signed zero, NaN, infinities and large finite values.

## Checkout reads

An existing `.git` entry selects exactly
`git --no-optional-locks rev-parse --verify HEAD^{commit}` in the requested
directory, including repositories whose `.git` is a file. A failed Git read
does not fall back to `build.json`. Missing or dangling `.git` paths use build
metadata; a symlink loop follows the source `Path.exists()` behavior.

Packaged metadata is decoded as UTF-8 and read through the existing native
Python-compatible JSON decoder. Duplicate keys retain their last value;
unrelated NaN, infinities, large integers and lone surrogates remain accepted.
Malformed data, missing/wrongly typed fields, invalid UTF-8 and read failures
return unavailable. Only ASCII hexadecimal strings of exactly 40 or 64
characters are accepted. Git output is stripped using Python whitespace
semantics; metadata commit strings are not stripped. Git stream decoding
matches the UTF-8 source locale used in validation.

## Captured execution boundary

The caller supplies the explicit installed `openpilot-process-child` path.
There is no Python execution, shell fallback or implicit helper lookup.
`CapturedCommand` uses the existing helper's direct execve and PATH search,
preserves cwd/argv, inherited stdin, process group and environment, and closes
unrelated inheritable descriptors. It does not replace MANAGER_DAEMON.

A private temporary directory contains the launch descriptor and Unix socket.
The helper's socket is close-on-exec. Parent launch waits for this transition
before checkout-status starts its one-second deadline, so helper startup does
not consume the Git execution budget. Stdout and stderr are drained together;
an error or timeout kills and reaps the owned child. The launch handle owns
temporary-directory cleanup. Callers of the generic captured API must observe
and reap its `Child`, as checkout-status does.

An early helper exit before connection is a protocol error. Socket EOF can
also accompany a fatal helper exit; callers still observe the real child
status. Failed target/cwd execution sends its native errno to the parent,
which reaps the helper and returns a spawn error. A successfully executed
target exiting with code 1 remains an ordinary child result. Both produce
unavailable at the checkout-status API. `spawn_inherited()` uses the same
execution boundary with inherited stdout/stderr for callers such as bootlog.
Helper artifacts and writable temporary storage are required native infrastructure.

For a bootlog caller, keep the returned handle alive until the child is reaped:

```rust,ignore
let mut child = openpilot_process_supervision::CapturedCommand {
    launcher: helper_path,
    cwd: logger_directory,
    argv: vec!["./loggerd".into(), "--bootlog".into()],
}.spawn_inherited()?;
let status = child.process.wait()?;
```

An unsuccessful `status` is an executed child result; a failed exec returns
`Error::Io` from `spawn_inherited()` with the underlying OS errno.
For bootlog's temporary Params snapshot, use
`spawn_inherited_with_env(&[("PARAMS_COPY_PATH".into(), snapshot_path.into())])`.
Overrides apply to the helper and its executed child without changing the
calling process environment, so a threaded caller need not mutate global state.

The existing native and persistent supervision modes keep their behavior.
Persistent descriptor closing and signal restoration are shared with captured
exec; the full supervision oracle covers this extraction.

## Validation

```sh
cargo test --manifest-path rust/Cargo.toml \
  -p openpilot-checkout-status -p openpilot-process-supervision --all-targets --locked
cargo build --manifest-path rust/Cargo.toml \
  -p openpilot-checkout-status -p openpilot-process-supervision \
  --examples --bin openpilot-process-child --locked
python rust/tools/check_checkout_status.py \
  --binary /path/to/checkout_trace --fixture /path/to/git_fixture \
  --launcher /path/to/openpilot-process-child \
  --captured-binary /path/to/captured_trace \
  --handshake-fixture /path/to/handshake_fixture --output /tmp/checkout-evidence
```

Seven suites compare the unchanged original module with native execution:
50 metadata cases; real SHA-1, SHA-256 and separate-Git-directory repositories;
17 executable cases; six packaged-state updates; and 44 Git-backed state
updates with actual invocation counts. Controlled executables exercise failed
status, invalid stream encoding, signals, missing/invalid executables, PATH
search, two large pipes, and the one-second timeout with open or closed pipes.
An actual held descriptor proves closure while stdin, environment and process
group remain inherited. Every operation records empty child and launch-file
lists after completion.

Fixture setup commands disable automatic Git maintenance with the per-command
`-c maintenance.auto=false` option. Git 2.55 detaches maintenance after the
synthetic commits, leaving children adopted by the test subreaper even when
checkout reads complete correctly. The original and Rust identity commands
retain their exact arguments, and child/descriptor cleanup assertions remain
strict. This fixture isolation is tracked in
[#109](https://github.com/bin9208/openpilot-rust/issues/109).

Eleven additional native helper cases cover successful EOF, failed cwd/exec,
missing or early-exiting helpers, corrupt descriptors, connected exit/SIGKILL,
invalid handshake data and temporary-storage failure. A 700 ms helper delay
followed by a 500 ms target verifies the timeout starts after exec. These are
native infrastructure tests, not fabricated Python helper behavior. Six
additional comparisons against actual `subprocess.call` verify inherited
stdout/stderr, descriptor closure, valid exits 0/1, ENOENT/ENOEXEC/EACCES and a
child-only `PARAMS_COPY_PATH` override with the parent environment unchanged.

Host and generic GNU aarch64/QEMU runs execute the same suites. ARM checkout,
captured-exec and helper binaries run under QEMU; managed Git executables and
the Git fixture are the same host binaries used by the original Python oracle.
The retained pre-pause checkout runs contain nine passing suites per target,
including the original five inherited-stdio cases. The pre-pause helper has 43
passing host supervision scenarios; the retained 43-scenario ARM supervision
run predates the inherited-stdio extension. The final child-environment change
is checked with the host helper suite; CI must repeat the broad host/ARM matrix.
Final fast checks and retained
artifact verification are in the issue worktree's
`.omo/evidence/checkout-status/resume-verification.json`; original invocations,
before-failure captures and cleanup observations remain in its referenced logs
and scenario directories. These retained runs are not exact-commit CI results.

Before captures retain two real defects in the initial port: stopping PATH
search after ENOEXEC, and leaking an inheritable descriptor. ARM tracing also
exposed glibc posix_spawn error reporting through QEMU's vfork emulation. The
direct-exec helper removes dependence on that boundary and on libc execvp's
possible shell fallback. No QEMU-specific production branch was introduced.

External dependencies remain Linux process/procfs/filesystem/Unix-socket APIs,
Git, nix/rustix libc boundaries, and existing native JSON/logging/Params/cereal
dependencies, including their current libzmq/CXX transport dependencies.
AGNOS ABI, vehicle behavior, complete manager startup and CPU savings are not
validated here. `manager_status` remains `not_ported`; no vehicle, GPIO or NAS
is used. Parent owns Actions, PRs and dev integration.

Docs-Not-Needed: internal optional manager prerequisite preserving production
startup, process selection and settings.
