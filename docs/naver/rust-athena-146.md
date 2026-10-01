# Native Athena runtime (#146)

Issue: https://github.com/bin9208/openpilot-rust/issues/146

`openpilot-athena` implements `openpilot-athenad` and
`openpilot-manage-athenad`. It owns continuous WebSocket sessions, JSON-RPC
handling, four upload workers, queue persistence, reconnect/backoff, ping and
TCP options, local proxy workers, log/stat forwarding, camera snapshots and
supervisor restart/cleanup. `manager-catalog` advertises the candidate; production
selection remains unchanged. This component is host-validated engineering work,
not the full-runtime startup/upload delivery gate in issue #1 or device evidence.

Source behavior comes from `openpilot/system/athena/athenad.py`,
`manage_athenad.py`, `openpilot/system/camerad/snapshot.py`, and their original
Params, Paths, upload-stream, logging, registration and process helpers at base
`d1754024`. The preimplementation comparison contract is
`rust/crates/athena/tests/contract.json` (commit `b66eb37a`). Test-only Python
oracles execute unchanged source function bodies. Runtime binaries do not invoke
Python. The msgq creation fix from `9939f816` is included before native IPC gates.

The original camerad executable is still a separate project-owned runtime
dependency; snapshot lifecycle ownership is ported, camera implementation is not
claimed here. Existing msgq/VisionIPC and libzmq remain native dependencies.
External protocols use pinned tungstenite 0.29.0, ureq 3.4.2, socket2 0.6.5,
rustls and zstd through Cargo.lock. JPEG uses the unmodified external
libjpeg-turbo 3.1.4.1 codec; exact origin, archive hash, licenses and boundary
ownership are in `rust/crates/jpeg/PROVENANCE.md`.

## Reproduction

Use Linux with C/C++ compilers, CMake, pkg-config/libzmq, strace, iproute2 and
unprivileged user/network namespaces. Python test dependencies are requests,
json-rpc==1.15.0, websocket-client, websockets==15.0.1, PyJWT[crypto], NumPy,
Pillow==12.3.0, pycapnp and zstandard. Tests use owned loopback servers, synthetic
ECDSA keys, private Params/log roots and private IPC prefixes. Proxy port 22 is
bound only inside a new network namespace; host SSH is never contacted.

Check free space before builds: retain 25 GiB plus estimated growth (1 GiB for
these bounded builds). Use an inactive coordinated target, disable incremental
compilation and two jobs. From the checkout root:

```sh
CARGO_INCREMENTAL=0 cargo build --manifest-path rust/Cargo.toml -p openpilot-athena -p openpilot-process-supervision --bins --examples --locked -j2
CARGO_INCREMENTAL=0 cargo test --manifest-path rust/Cargo.toml -p openpilot-athena -p openpilot-jpeg -p openpilot-manager-catalog --locked -j2
CARGO_INCREMENTAL=0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-athena -p openpilot-jpeg --all-targets --locked -j2 -- -D warnings
python rust/tools/check_athena_runtime.py --bin-dir "$CARGO_TARGET_DIR/debug" --output .omo/evidence/athena-146/final
python rust/tools/check_jpeg_sanitizers.py --target-dir .analysis/scratch/jpeg-sanitizer-target --output .omo/evidence/athena-146/jpeg-sanitizers
```

`check_athena_runtime.py` records every exact invocation, exit code and captured
log in `suite.json`. Each runtime scenario writes `result.json`; source matrix
checks write the named JSON output. A nonzero scenario fails the driver.

## Evidence and observable acceptance

Artifacts are under `.omo/evidence/athena-146/` in the implementation worktree;
the root receipt records its absolute location and SHA-256 manifest. The final
suite is under `final/`. No captures or synthetic credentials are committed.

| Scenario / checker | Binary observable | Artifact |
| --- | --- | --- |
| source policy, `check_athena_policy.py` | 27 traces equal: heap order, SHA-1 IDs, cache/partial invalid restore, queued-only cancellation and retries | `final/policy.json` |
| source RPC, `check_athena_rpc.py` | 53 routing/error/notification/JSON cases equal | `final/rpc.json` |
| complete cereal, `check_athena_ipc.py` | 11 source-schema comparisons equal, including default active unions | `final/ipc.json` |
| source NV12/JPEG, `check_athena_image.py` | five matrices, RGB and JPEG bytes equal; includes all 65,536 UV pairs at four Y values | `final/image/result.json` |
| native daemon, `check_athena_daemon.py` | signed auth, fragments, ping Params, real uploads/retry, reconnect and exit zero | `final/daemon/result.json`, `sockets.log` |
| native transfers, `check_athena_transfers.py` | real IPC, four held workers, cancellation, dedup, metered deferral, connection-vs-body failure behavior, TCP/TOS | `final/transfers/result.json`, `sockets.log` |
| source/native HTTP, `check_athena_upload_edges.py` | status matrix, 301/302/303/307/308 methods/bodies, compressed bytes equal; expiry and retry30 no extra request | `final/upload-edges/result.json` |
| metered transition, `check_athena_metered_abort.py` | partial 12 MiB transfer aborts, retry count stays zero, resumed bytes exact | `final/metered-abort/result.json` |
| forwarding, `check_athena_forwarding.py` | source scan/cache/one-hour boundary equal; real log/stat RPC, ACK xattrs and temp-file retention | `final/forwarding/result.json` |
| snapshot, `check_athena_snapshot.py` | frame79 waits/frame80 completes, actual VisionIPC, both JPEGs exact, RecordFront/concurrency/SIGTERM cleanup | `final/snapshot/result.json`, JPEGs |
| camera lifecycle, `check_athena_camera_lifecycle.py` | native process child starts/reaps owned camera; existing camera preserved; Params cleared | `final/camera-lifecycle/result.json` |
| supervisor, `check_athena_supervisor.py` | SIGKILL restart after five seconds, inherited logging context despite metadata mutation, SIGTERM child reaping and PID cleanup | `final/supervisor/result.json`, `records.jsonl` |
| proxy, `check_athena_proxy.py` | 22/8022 map, rejected23, signed WebSocket, exact 1 MiB duplex bytes, TOS144, global stop | `final/proxy/result.json`, `sockets.log` |
| reconnect, `check_athena_reconnect.py` | 503 recovery clears old ping; real 30-second reads enforce >70-second ping timeout at about90seconds | `final/reconnect/result.json` |
| JPEG sanitizer, `check_jpeg_sanitizers.py` | complete C codec plus C/CXX boundary instrumented; ownership/rejection test passes, no sanitizer/leak errors | `jpeg-sanitizers/result.json`, `run.log`, `build.jsonl` |

Earlier red artifacts retain the discovered RPC1 null-ID behavior, default-union
serialization, closed keepalive retry and pre-response EOF classification errors
alongside subsequent passing scenarios. JPEG-encoder produced different decoded
pixels and was replaced with the source codec; no image tolerance was loosened.

The evidence does not establish C3X behavior, physical camera results, cellular
hardware behavior, measured CPU savings, or complete runtime conversion. No
vehicle connection, vehicle test, production service, NAS replay deployment or
production daemon selection is part of this change. Full normal startup and the
existing upload path remain the integration gate before the user's first device
comparison. Docs-Not-Needed: experimental runtime implementation adds no setting
or public-user workflow; no user guides or Wiki content are changed.

`git diff --check` is clean for project-owned changes. The unchanged upstream
vendor archive contains Markdown seven-equals headings and generated CSS/JS
whitespace that this check flags; `jpeg-vendor-integrity.json` verifies all 633
files against the release archive, and `diff-check.log` retains those findings.
