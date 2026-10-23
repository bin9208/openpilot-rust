# Panda safety source timeout diagnostic (#518)

[Issue #518](https://github.com/bin9208/openpilot-rust/issues/518) follows
[job120220822773](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/job/120220822773)
at source `10919538d31b04d28d9a23b383d925441434d579`. Native builds, tests,
lint and 3,174 original protocol cases passed. The subsequent original C++
safety batch reached its 90-second deadline before native safety ran.

Read-only examination authenticated artifact12485436178, ZIP SHA-256
`965038890303673a38af53376f8b315fdc7f96df20cc5bf1e85d9676d1f21905`.
Its retained Params match input row 864, `seeded-17`, operation 12, after the
truncation/malformed families. The 897 rows contain 4,499 operations and 6,242
explicit put requests. Each successful original put performs file and directory
fsync. Cumulative durable I/O is a hypothesis; syscall latency was not measured.
The original timeout discarded partial stdout/stderr. No pipe deadlock,
filesystem root cause or successful native safety comparison is established.

## Approved one-off design and implementation plan

Only `codex/fix-518-panda-safety-probe` schedules this workflow. It has no PR and
changes no PR500 runtime or existing Panda acceptance. All execution belongs on
registered `bin-actions` (`self-hosted`, `Linux`, `X64`, `dev-md-local`). Checkout
uses `clean: false`; standard job-temp owns output and cleans it after uploads.
Fast excludes only this branch's push to preserve existing continuation state;
its PR trigger, normal jobs and required names remain unchanged.

1. Authenticate the exact original artifact through GitHub REST with
   `actions: read`. Never forward API Authorization to signed storage and never
   print/store signed URLs. Admit bounded download/extraction/evidence growth
   before download. Bound archive size, entries, total expanded size and selected
   members. Extract only the pinned binary, input and build manifest.
2. Pin binary SHA-256
   `8f019be68c4d19311344d9db016b1ed36846905145345b33eda865f3a8a9e7f0`
   and original input SHA-256
   `20af4658f3f42d016780e7f2ba4bc7ea8afc1ac1802cddbb464c203eee75c11b`.
   Record manifest and dependency resolution with `ldd -r` in Actions. Missing
   dependencies fail closed; do not silently install a replacement ABI or
   rebuild previously successful unrelated packages. A necessary rebuild would
   use only the existing safety-oracle builder after the reuse blocker is known.
3. Relocate only `params_root`, separately for each observation. Verify all other
   fields and row order, 897 rows and 4,499 operations. Preserve the complete
   within-row lifecycle and eight truncation/recovery inputs. Record original,
   relocated and root-independent canonical input digests. Require real Params
   storage on the workspace filesystem; reject tmpfs/ramfs substitution.
4. Capture one normal original-source observation with the existing 90-second
   execution deadline. GNU timeout owns termination and a fresh process group;
   a separate bounded parent wait kills only that group if necessary. Service
   stdin/stdout/stderr concurrently. Preserve bounded partial stdout/stderr,
   complete JSON output-row/operation counts and input labels, elapsed time,
   process-tree resource usage and bounded read-only owned-child proc samples.
   Proc reads use held directory descriptors, process-group checks and the
   authenticated executable inode. Retain the five synthetic Params keys as a
   bounded JSON snapshot for a possible mid-row input-write boundary.
5. Only if normal still times out, collect a second 90-second observation under
   `strace -f -c -w` restricted to durability/lock/rename/unlink syscalls. Prefer
   installed strace; only if missing, admit operation growth and install it in
   Actions through unchanged `apt-native`. Profile stderr includes the tracer
   summary and source stderr. Mark instrumentation overhead; retain inherited
   sanitizer settings. LSan/ptrace incompatibility is an observed profile
   limitation, never a reason to disable leak checking or claim a pass.

## Verification contracts, written before implementation

- Wrong artifact metadata/digest, binary/input digest, archive member bounds,
  input shape/count or filesystem ownership prevents source execution.
- API redirects are handled explicitly; storage requests carry no Authorization.
- No original input field except the owned Params root changes. Observation
  roots are fresh and differ, so a profile cannot reuse normal's Params state.
- Both observations retain the 90-second source deadline and bounded cleanup.
  No Popen context-manager exit introduces an unbounded wait.
- A timeout preserves received bytes and records the last complete input-label
  boundary. Truncated output and incomplete cleanup are explicit failures of
  evidence collection, not successful comparisons.
- A normal run that completes does not force a new timeout: profile is skipped
  with an explicit reason. Nonzero exits and sanitizer errors are retained as
  observed. No synthetic failure is fabricated.
- `diagnostic_only: true` and `source_native_acceptance: false` are present in
  receipts regardless of source outcome. A green diagnostic job means evidence
  collection completed; all existing source/native assertions, padding controls,
  transport/supervisor checks and required gates remain separate acceptance.

Local verification is Python AST, YAML/Bash syntax and diff review only. No
helper/project imports, fixtures, artifact binaries, tests, builds or sudo are
executed locally. Actual authentication, dependency resolution, source timing,
partial capture, trace availability and upload/cleanup remain pending Actions.
This preparation does not establish device, hardware or complete Panda safety
acceptance. No commit, push or dispatch is part of this implementation handoff.
