# Runtime bootstrap network readiness (#278)

Issue: [#278](https://github.com/bin9208/openpilot-rust/issues/278). Host implementation verified on 2026-10-14; device acceptance remains separate.

Base: `9200ffe57c385e2fa4a89ff11821c7e9ac710d2f` (`origin/dev`). Branch: `codex/fix-278-runtime-bootstrap-network`. PR and exact-source Actions validation follow these local checks; no Actions result is claimed in this record.

## Failure and correction

`ensure_runtime.sh` previously used three curl retries followed by `set -e` termination. Native transport likewise stopped after three immediate attempts, including permanent failures. Host regressions directly reproduced failure before the fourth failed transfer could recover. Incident context supplied by the parent placed installer startup before WLAN/time readiness; this worker did not inspect the device captures. Original curl stderr was unavailable, so the particular DNS/TLS failure is not established.

Both download paths now retry curl DNS/connect/partial-transfer/timeout/TLS/send/receive errors (`5,6,7,18,28,35,52,55,56,60`) after five seconds. HTTP `408,429,500,502,503,504` also retry; HTTP 404 and other terminal curl errors fail immediately. Native HTTP status parsing uses bounded byte lines and accepts non-UTF-8 unrelated header values. Retry diagnostics go to stdout and curl error details remain on stderr. Retries have no total attempt cap, allowing late network or clock recovery. A persistently invalid certificate can therefore keep startup waiting; certificate verification is never bypassed.

Each retry starts from zero and removes incomplete output. Bootstrap SIGINT/SIGTERM cleanup kills and reaps current Bash jobs before deleting staging. A regression exposed a narrow signal race before the sleep PID assignment; cleanup uses the job table to cover it. Native transport waits/reaps each normal curl attempt and retains the existing cancellation/reap path for copy/bound failures. File/size setup occurs before spawning curl.

Exact-source URL selection, HTTPS-only redirects, SHA-256/declared-byte checks, source/AGNOS/architecture identity, package extraction validation, advisory locks, and atomic generation activation remain required. Installed packages and verified cached installers keep their offline path. No setting, OS update policy, runtime scheduling, dependency, or vehicle-control threshold changed.

## Reproduction and verification

Artifact root: `.omo/evidence/runtime-bootstrap-278/` in this implementation worktree. The local [evidence ledger](../../.omo/evidence/runtime-bootstrap-278/evidence.md) records exact invocations, scenario names, binary observables and receipts. Artifacts are local ignored data; the tracked record is this file.

| Scenario | Binary observable | Artifact |
| --- | --- | --- |
| Shell DNS/connect/certificate/partial failure sequence `6,7,60,18` | Exit 0 after at least 20 seconds, installed marker written, six requests, no abandoned download directory | `runtime-packaging-green.xml`, `cases/late-network-and-clock/observable.json` |
| Shell four HTTP 503 failures | Exit 0 after at least 20 seconds, installed marker written | `runtime-packaging-green.xml`, `cases/temporary-http503/observable.json` |
| Native four real truncated HTTP bodies | Fifth response returns exactly `complete`; elapsed >=20 seconds | `installer-green.log` |
| Native four real HTTP 503 responses with a non-UTF-8 header | Fifth response returns exactly `complete` | `installer-green.log` |
| Missing release / oversized body | Native HTTP 404 and oversize stop after one request; shell missing checksum exits 22 preserving old runtime | `installer-green.log`, `runtime-packaging-green.xml` |
| Helper/package integrity failures | Helper is never executed; wrong package digest/artifact hash never activates runtime | `runtime-packaging-green.xml`, `installer-green.log` |
| SIGTERM during curl or retry backoff | Exit 143, curl PID reaped, no installer execution or staging directory | `runtime-packaging-green.xml`, `cancellation-stress-green.log` |
| Installed/cached offline paths | Shell makes zero requests; native installed generation reused after local inputs removed | `runtime-packaging-green.xml`, `installer-green.log` |

RED receipts: `bootstrap-red.log` (exit 6 and cancellation timeout), `installer-red.log` (three-attempt exhaustion), `http-bootstrap-red.log` / `http-installer-red.log` (503 terminal), `backoff-race-red.log` (signal race), `non-utf8-header-red.log` (valid HTTP header rejected by UTF-8 decoding). Final GREEN: 81 tests in the complete runtime packaging/bootstrap CI selector; 34 tests in the full installer crate suite (5 transport +29 real CLI); 20 repeated cancellation invocations /40 scenarios. Bash syntax, rustfmt, installer clippy `-D warnings`, focused Ruff, mapped user-docs validator, and diff whitespace checks pass.

Each cargo invocation recorded free space first (about 64 GiB), reused `/home/bin9/openpilot-rust/rust/target/runtime-269`, and disabled incremental compilation. No dependencies were installed. Native transport is 241 pure lines including its existing inline tests; another substantive expansion should move the tests to a dedicated module.

## Limits and next gate

The shell fixture substitutes curl at its fixed GitHub boundary and simulates certificate exit 60; native transport tests use real host curl and owned loopback HTTP servers. No actual target clock transition, DNS/TLS incident, aarch64 build, device startup, or vehicle acceptance is established by these host tests. SIGKILL cleanup and individually signaling the native installer during a live curl transfer are not claimed; bootstrap removes abandoned download directories on the next start, while native package staging/recovery protections remain unchanged. Parent task owns device validation, review, PR and Actions verification.

`Docs-Not-Needed: installer network/time recovery adds no setting or experimental CLI option; no user-guide edits were requested.`
