# Native legacy uploader orchestration

Issue [#50](https://github.com/bin9208/openpilot-rust/issues/50), under the full
runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1). Source provenance:
`openpilot/system/loggerd/{uploader,xattr_cache}.py`, `openpilot/common/{api,utils}.py`,
`openpilot/system/version.py`, and the original Params/deviceState interfaces.

`openpilot-uploader` runs the upload loop in Rust. Its source manager registration
is commented out in `process_config.py`; this port does not enable it. The active
Carrot upload path and complete normal-startup candidate remain separate work.

The port retains the source directory ordering, stable per-file priority, lock
exclusion, cached upload attributes, metered requested-route exception, compression
decision, exact size-cap names, accepted response statuses and retry/backoff state.
It reuses the deleter's already-validated directory sort without changing that
algorithm. Boot/crash selection examines the full pathname, including the source's
substring behavior. The source's ineffective bare-directory versus trailing-slash
12-hour comparison is preserved.

The HTTP path loads persisted RSA before EC keys, requires both private/public
files, signs the original one-hour JWT claims, requests the v1.4 upload URL, and
streams the selected file or a level-10 zstd buffer. Native conversion supports
traditional SEC1 EC PEM as well as PKCS#8. API status 412 bypasses JSON parsing and
PUT. FAKEUPLOAD still signs and requests the URL. HTTP 200, 201, 401, 403 and 412
remain successful marking outcomes. Redirects replay seekable bodies for 307/308
and retain the source's other method/body transitions. The pinned ureq transport
adapter applies the source's ten-second I/O timeout without imposing a ten-second
total transfer deadline; TLS verification remains enabled. Socket operations also
check monotonic elapsed time after I/O: Linux can round a socket deadline up and
otherwise accept a response arriving just after the source deadline.

`LOG_ROOT`, HOME/prefix persistence paths, Params, API_HOST, FORCEWIFI presence,
FAKEUPLOAD presence and UPLOADER_SLEEP retain normal startup roles. The executable
starts from the repository root so the version header resolves as in the normal
startup layout. FORCEWIFI overrides availability only; raw deviceState network
type and metering still reach the uploader. Startup clears direct segment locks
before requiring DongleId. Empty/invalid UTF-8 DongleId is treated as missing.
Upload events and errors use the native structured logging producer. SIGINT and
SIGTERM interrupt idle waits with 20 ms checks; in-flight HTTP operations retain
their per-I/O timeout behavior. `--cycles N` bounds host test iterations.

The confirmed original empty/oversized-file marking failure is tracked separately
in [#51](https://github.com/bin9208/openpilot-rust/issues/51). Original Python raises
`UnboundLocalError` when its xattr-error handler reads uninitialized `last_exc`.
Rust returns `UninitializedLastException` and exits nonzero. This parity port does
not silently change marking/retry semantics. Normal transferred-file marking
errors continue to be logged without changing the successful return.

## Validation surfaces

- Differential execution of actual source uploader, xattr cache, API/JWT and
  compression helpers: 212 filesystem/status/error scenarios and 10,000 exact
  backoff decisions. Read-only empty and oversized temporary files exercise the
  real failing xattr syscall as well as the typed native failure.
- 32 local HTTP scenarios verify RS256/ES256 signatures and claims, exact compressed
  request bodies, 412 short-circuiting, fake uploads, rejected status/JSON,
  redirects, timeout failure and slow-but-progressing response bodies.
- Ten unscaled timeout scenarios exercise GET/PUT headers and bodies delayed
  10.05 or 10.6 seconds, plus eleven-second response bodies with sub-timeout
  progress. Actual Python and Rust agree on results and upload xattrs. The
  pre-fix native binary incorrectly accepted two late API responses; its failing
  trace is retained separately in the follow-up evidence.
- Six continuous runtime scenarios connect original msgq deviceState publishers
  to Rust, use disposable Params/HOME/log roots, collect original-compatible ZMQ
  log packets, and inspect actual HTTP request bodies and xattrs. They cover Wi-Fi,
  metered route requests, forced availability with raw network type none,
  FAKEUPLOAD, and both idle shutdown signals. Three startup failures return nonzero.
- Unit tests use real filesystem xattrs and locks. GNU aarch64 cross-build and the
  same continuous scenarios under qemu validate host emulation only.

No real signing keys, existing vehicle logs, external upload endpoints or vehicle
connections are used. Temporary fixture keys are not committed. The runtime uses
original msgq through CXX, libzmq, native zstd, and third-party Rust HTTP/TLS/crypto
libraries; it does not invoke Python or project shell helpers.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-uploader --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-uploader --bins --examples --locked
python rust/tools/check_uploader.py \
  --binary rust/target/debug/examples/uploader_trace --output /path/to/reference.json
python rust/tools/check_uploader_timeouts.py \
  --binary rust/target/debug/examples/uploader_trace --output /path/to/timeout-evidence
PYTHONPATH=/path/to/original-msgq-binding:.:rust/tools python rust/tools/check_uploader_daemon.py \
  --binary rust/target/debug/openpilot-uploader --output /path/to/fresh-runtime-evidence
```

Python validation dependencies are pinned in the Rust CI workflow. CI preserves
the existing host/aarch64 requirements and adds the source/HTTP/IPC checks. Local
artifacts are indexed in `.omo/evidence/uploader/evidence.json` in the issue-50
worktree. The timeout follow-up is indexed separately in
`.omo/evidence/uploader-timeout/evidence.json`; cloud validation belongs to the
integration handoff. These results do not establish device acceptance, CPU savings, or full-runtime completion.

Docs-Not-Needed: optional internal runtime port preserving existing settings and
the original disabled process registration.
