# Native registration startup policy (#85)

`rust/crates/registration` translates the project policy in
`openpilot/system/athena/registration.py` and the registration-relevant key and
request helpers in `openpilot/common/api.py`. The inherited sources and their
license remain unchanged. This increment starts from `ce1f59107937a22ff14adfc7b306bca037e9a1b9`
on `codex/feat-85-registration`, under the approved [runtime design](design.md).

## Scope and preserved boundaries

`Registration::register` uses native Params storage and the typed STRING getter,
including the original invalid-UTF-8 warning. A missing Params identity can fall
back to the stripped persist file. The fallback's filesystem errors propagate;
key existence checks retain `os.path.isfile` behavior. RSA precedes EC, both key
files open before either is read, and text reads use universal newlines. Key
loading still happens for an existing identity. A missing or empty public key
replaces it with `UnregisteredDevice` and emits the existing warning.

The hardware and spinner interfaces preserve call ordering without invoking
project Python. A second IMEI-read exception discards the first result, an
exception sleeps one second, and two `None` results poll again without sleeping.
One non-`None` result, including an empty string, ends polling. Serial lookup and
spinner startup failures propagate without a finally-close operation. Detailed
spinner text appears only after strictly more than sixty seconds.

The registration token is really signed as RS256 or ES256, including traditional
SEC1 and PKCS8 EC keys. Its only claims are `register: true` and UTC expiry one
hour later. SystemTime conversion floors negative fractional timestamps and
preserves Python's years 1..9999, including overflow when adding the hour.
Thirteen controlled SystemTime cases compare against Python without changing
the host clock; signed request fixtures also cover pre-epoch and calendar edges. The existing uploader signer now exposes a PEM constructor and a
claims method; its original identity-token and key-loading paths are retained.

The API call puts the IMEIs, serial, public key and signed token in the POST query,
omits `None` values and sends an empty body with the existing version User-Agent.
It uses the shared fifteen-second **per-socket-I/O** deadline, permits thirty
redirects, changes POST to GET for 301/302/303, and retains POST for 307/308.
The request owns a fresh session on every authentication attempt. Cookie path,
domain, expiry, secure flag and redirect applicability are handled within that
session. The localized correction for [#88](https://github.com/bin9208/openpilot-rust/issues/88)
prevents ureq 3.4.2 from sending Set-Cookie attributes in request Cookie headers.
It does not alter the existing uploader transport. Responses support the
inherited advertised gzip, deflate and brotli encodings, with text decoding at
the response boundary before JSON parsing. `application/json` defaults to UTF-8
and text media types to Latin-1, matching Requests. Only an otherwise undeclared
encoding uses the pinned native detector described in
[charset-norm-provenance.md](charset-norm-provenance.md). The older native detector
was rejected after it silently changed identifiers in source-accepted fixtures;
no fixture-specific heuristic or toolchain upgrade is used.

402 and 403 choose `UnregisteredDevice`. Other statuses still parse JSON rather
than calling raise-for-status. Parsing retains Python null, booleans, arbitrary
integers, nonfinite floats and string code points through the existing native
JSON library. False values return without writing; a truthy nonstring or lone
surrogate fails **after** the spinner closes. Cython's STRING conversion can
raise, but its Params writer discards the C++ I/O return code; the local adapter
preserves that distinction. Authentication exceptions log and sleep 1, 2, ...,
15, 15, ... seconds. Logging failures retain their original catch boundaries.

## Evidence and reproduction

The issue worktree's `.omo/evidence/registration/evidence.json` is the evidence
ledger. The source matrix contains 103 scenarios on x86-64 and generic aarch64
under QEMU. Three additional real-time scenarios enforce the fifteen-second
header/body deadline and allow a progressing sixteen-second response. The
unchanged uploader oracle passes 212 filesystem scenarios, 10,000 backoff
decisions and 32 signed HTTP/compressed-payload scenarios. It records commands, binary hashes, source fingerprints, source/native
HTTP bytes, Params outcomes, hardware/clock/spinner traces, original collector
cereal packets and persisted Swaglog records. JWT encodings can differ in JSON
key order; each captured token is independently signature-verified and its
complete claims are compared. Native diagnostic callsites/provenance identify
Rust, while messages, severity and exception presence match the source. Failed
comparisons are retained:
redirect-cookie attributes, secure cookies over loopback HTTP, filesystem error
boundaries, deflate and UTF-16 decoding. The collector fixture wakes a blocked
receive after SIGINT so the original Python signal handler can run; its captured
shutdown investigation is a harness limitation, not a registration runtime fix.

The oracle runs unchanged source function definitions. Only external hardware,
spinner and clock inputs are synthetic. Params uses the actual original Cython
extension; the collector uses the unchanged original daemon and msgq bindings.
RSA and EC keys are generated fixtures, never host or device keys. Python 3.12,
pycapnp 2.1.0 and NumPy 2.5.3 are pinned, with Requests/PyJWT/cryptography versions
recorded from the repository lock. All HTTP listeners are ephemeral loopback.

```sh
cargo fmt --manifest-path rust/Cargo.toml -p openpilot-registration -p openpilot-uploader --check
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-registration -p openpilot-uploader --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml -p openpilot-registration -p openpilot-uploader --all-targets --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-registration -p openpilot-uploader --examples --locked

# PYTHONPATH begins with the built original msgq bindings, then the repository and rust/tools.
python rust/tools/check_registration.py REGISTRATION_TRACE ORIGINAL_PARAMS_SO OUTPUT
python rust/tools/check_registration.py REGISTRATION_TRACE ORIGINAL_PARAMS_SO TIMEOUT_OUTPUT --timeouts
python rust/tools/check_uploader.py --binary UPLOADER_TRACE --output UPLOADER_REPORT
python rust/tools/check_registration_clock.py REGISTRATION_CLOCK CLOCK_REPORT
python rust/tools/check_registration_vendor.py CHARSET_NORM_ARCHIVE VENDOR_REPORT

cargo zigbuild --manifest-path rust/Cargo.toml -p openpilot-registration --example registration_trace --target aarch64-unknown-linux-gnu.2.28 --locked
python rust/tools/check_registration.py ARM_REGISTRATION_TRACE ORIGINAL_PARAMS_SO ARM_OUTPUT \
  --runner QEMU_AARCH64 --runner=-L --runner ARM_SYSROOT
```

## Remaining integration gates

This is an internal startup library with a callable real-clock adapter. Actual
board/serial/modem discovery and the visual spinner are deliberately unported
interfaces. Native dependencies include libzmq, filesystem/clock APIs and Rust
HTTP/TLS, cryptography, text and cookie libraries; the existing C++ msgq boundary
remains in the linked support crates. Full `Api` convenience methods unrelated
to registration are outside this increment.

Manager selection, normal AGNOS startup, full-runtime log-upload comparison and
the user's first device test remain pending. No vehicle, real persist key,
public registration service, modem or actual spinner UI is accessed. Generic
ARM/emulator evidence does not establish device behavior or CPU savings. This
increment is not a complete runtime candidate, and neither issue #85 nor #88
is closed by local validation; parent exact-SHA integration and CI remain separate.
