# Native Carrot web discovery reporter

Issue [#142](https://github.com/bin9208/openpilot-rust/issues/142), full runtime [#1](https://github.com/bin9208/openpilot-rust/issues/1). Source: unchanged `openpilot/selfdrive/carrot/cweb_push.py` from the independent repository baseline; licensing and original history remain intact.

`openpilot-cweb-push` owns IPv4 selection/filtering, device identity fallbacks, report debounce, retry scheduling, heartbeat jitter, payload/status formatting and continuous/one-shot/dry-run CLI behavior. It preserves the original environment variables and source defaults. Report failures wait the previous backoff while logging the next doubled value; restoring the same IP follows the source heartbeat path. Existing Params types, defaults, sentinel IDs and invalid UTF-8 behavior are preserved. No production process is switched by the catalog entry.

The source's urllib client does not decode unsolicited compressed responses, so this component uses the already pinned reqwest0.13.5 with decompression, automatic redirects and automatic retries disabled. Rust implements urllib POST/GET redirect policy and loop limits, retaining per-read/connect deadlines and raw UTF-8 replacement decoding. HTTP method, path, JSON body and explicitly configured headers are source-compared. The native library adds its implicit `Accept: */*`; this is not claimed as byte-identical complete headers. Native transport exception descriptions may differ outside the source-stable status/body cases. No new registry version is introduced.

Normal invocation discovers the interface/route address and runs continuously. The test-only `--fixture-ip FILE` redirects address discovery to an owned file and requires HTTP loopback report/heartbeat URLs. SIGINT/SIGTERM interrupt sleep and an in-flight HTTP future. A regression first proved the old candidate remained waiting for a stalled HTTP response after SIGTERM; the fixed candidate exits promptly without inventing a report failure or restarting the request. Source default SIGTERM exit and native graceful exit are explicitly distinguished in execution artifacts.

## Verification

- Unchanged source policy and helpers:32 cases/16,000 frames agree exactly, including IP transitions, debounce/retry boundaries, heartbeat/dry-run behavior, Unicode response clipping and URL/identity/IPv4 helpers.
- Actual source HTTP:20 cases pass status200/201/204/299/300/304/400/401/403/404/500, redirects301/302/303/307/308, loop limits, raw gzip bytes, malformed UTF-8, timed-out and progressing responses. Request JSON bytes and configured headers agree.
- Actual native and original CLI: six scenarios pass one-shot reporting, dry-run, absent IP, invalid stored UTF-8, continuous failed report/retry/heartbeat/address changes, and termination during a stalled response. The source uses its unchanged Params extension; native executable/maps are inspected. All endpoints and Params are synthetic/private.
- Linux address adapter: source/native read-only loopback/nonexistent-interface results agree. The identical production ioctl implementation passes ASan/UBSan with positive address, name truncation, socket failure and ioctl failure fixtures; descriptor cleanup is asserted. The CXX adapter only passes borrowed bytes synchronously and copies the returned string.

Artifacts are retained under `.analysis/scratch/2026-10-01-rust-cweb-push/`: `policy-source-1/`, `http-source-1/`, `daemon-final/`, `stop-http-red.log`, `address-2/` and focused build/lint logs. The second machine interruption damaged an incomplete Clippy log; recovery found no NUL/empty source damage in this component, Git fsck passed, and the affected checks were rerun with separate logs.

```sh
export CARGO_INCREMENTAL=0
cargo build --manifest-path rust/Cargo.toml -p openpilot-cweb-push --bins --examples --locked -j2
PYTHONPATH=.:rust/tools python rust/tools/check_cweb_policy.py --binary TARGET/debug/examples/cweb_trace --output EVIDENCE/policy
PYTHONPATH=.:rust/tools python rust/tools/check_cweb_http.py --binary TARGET/debug/examples/cweb_http --output EVIDENCE/http
PYTHONPATH=.:rust/tools python rust/tools/check_cweb_daemon.py --binary TARGET/debug/openpilot-cweb-push --binding PARAMS_BINDING.so --output EVIDENCE/daemon
PYTHONPATH=.:rust/tools python rust/tools/check_cweb_address.py --binary TARGET/debug/examples/cweb_address --output EVIDENCE/address
```

Check free disk before building or installing dependencies. The source Params extension comes from `build_params_python.py`; its oracle needs Python3.12/pyzmq/numpy and the existing native binding. Address sanitizers require Clang. External dependencies remain Linux sockets/ioctl, CXX, reqwest/Hyper/Rustls/platform certificates, Tokio and native Params/logging. No Python runs inside the native candidate. Real report service, target networking/AGNOS, device timing and complete startup/upload acceptance are unvalidated. The user's first device comparison remains after the full project-owned runtime conversion.

Docs-Not-Needed: experimental native implementation of existing behavior; no production setting or public guide changes.
