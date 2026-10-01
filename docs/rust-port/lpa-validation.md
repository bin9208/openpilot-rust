# Native TICI eSIM LPA

Issue [#115](https://github.com/bin9208/openpilot-rust/issues/115), full-runtime tracker [#1](https://github.com/bin9208/openpilot-rust/issues/1).
Source: unchanged `openpilot/system/hardware/tici/lpa.py` and `Profile`/`LPABase` in `openpilot/system/hardware/base.py`. Original MIT licensing and source provenance are retained. Implementation: `rust/crates/lpa`, library plus internal `openpilot-lpa` JSON operation entrypoint.

The library retains persistent serial transport, the blocking `/dev/shm/modem.lock` flock, ISD-R channel acquisition/closure, one reconnect after a serial failure, ten open attempts with a reset after attempt six, three APDU attempts, 120-byte ES10 segments, continuation responses, profile parsing/listing/nickname/deletion/switching, comma-profile deletion refusal, busy-profile reset/retry, eUICC detection, and the source's currently empty active-profile result. The CLI accepts one typed operation on stdin; optional `--config` supplies owned paths for offline fixtures. Defaults address the actual device and are not used by host tests.

Download retains time validity checks, challenge/info exchange, ES9+ authentication, metadata parsing, confirmation-code hashing, BPP splitting/installation/error messages and best-effort eUICC/server cancellation after a transaction exists. Notifications retain per-item failure continuation, HTTPS posting and removal only after successful delivery. The original GSMA CI bundle is embedded as the exclusive trust roots; certificate and hostname verification remain active. Native HTTPS applies the source's 30-second socket timeout, session cookie lifecycle and redirect handling. As with registration, the existing ureq 3.4.2 cookie-attribute serialization issue is handled with matched name/value cookies. The source's clock policy is reused from the native timed crate.

The original `/usr/comma/lte/lte.sh start` remains an external native reset command, launched through the descriptor-closing process helper. Linux termios/flock, the certificate bundle, shared timed support and the process helper's existing cereal/msgq/libzmq dependencies remain explicit. No production Python fallback or daemon-selection change is introduced.

## Focused host gate

Use the existing Python source-oracle environment with pyserial, requests and pycapnp, plus OpenSSL for temporary fixture certificates. Rust dependencies retain prior lockfile versions.

```sh
RUSTUP_TOOLCHAIN=1.94.0 cargo build --manifest-path rust/Cargo.toml -p openpilot-lpa -p openpilot-process-supervision --bins --examples --locked -j2
RUSTUP_TOOLCHAIN=1.94.0 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-lpa --all-targets --locked -j2 -- -D warnings
python rust/tools/check_lpa.py --target rust/target --evidence /tmp/lpa-evidence
```

The gate imports the actual source and substitutes only owned device/lock/reset/CA locations and shorter fixture timeouts. Six source/native scenarios compare exact return/error rows, AT/APDU transcripts, reset calls, and HTTPS payloads/headers/cookies. Only random loopback addresses are normalized:

- Profile lifecycle: list/active, Unicode nickname and byte limit, busy enable/reset, protected/missing/ordinary delete, eUICC detection, AT error and timeout.
- Download: TLV/TBCD/base64/BPP splitting, complete authenticated install with nickname, multi-segment APDU and continuation reads, notifications continuing past a malformed entry, confirmation-code hash and missing-code refusal.
- Server refusal: original friendly error and both eUICC/server cancellation.
- Channel-open retry: six failures, one reset, subsequent recovery.
- APDU retry/reconnect: two malformed modem responses, recovery, then real owned-PTY replacement and serial reconnection.
- Install failure: eUICC duplicate-profile result, friendly error, and transaction cancellation.

The native binary lifecycle holds the flock before startup, verifies its `/proc/PID/exe`, asserts no modem commands while blocked, releases the lock, checks profiles/channel closure/exit zero, and reacquires the lock. A separate loopback TLS scenario confirms source and native rejection of both the wrong hostname and an untrusted root before any HTTP request reaches the server. A stalled HTTPS response separately verifies both source and native socket read deadlines. Synthetic fixture profile/transaction/certificate data never contacts a carrier or physical modem/SIM.

Artifacts: `results.json`, each scenario's `*-source.json`/`*-native.json`, stderr captures, `lifecycle.json`, `tls-rejection.json`, `http-deadline.json`, build/clippy/fmt/Ruff logs. Local evidence lives at `.omo/evidence/lpa-115/` in the parent workspace. CI must build the `openpilot-process-child` helper alongside the LPA binary and trace example.

## Limits

These host results establish protocol/adapter behavior against synthetic peers. They do not establish physical modem/eUICC operation, real carrier provisioning, compatibility with every carrier's TLS chain, AGNOS ABI/device behavior, complete-runtime startup/upload or performance improvements. Generic ARM and exact-SHA Actions validation belong to integration. Production hardware callers remain unconverted until the owning integration replaces them. No device test was run or requested; the first comparison remains gated on the complete runtime candidate.
