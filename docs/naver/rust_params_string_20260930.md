# Native STRING Params boundary (#64)

The `openpilot-params-typed` crate implements the nonblocking STRING conversion used
by Python `Params.get(key)` with its default `return_default=False`. The lightweight
raw `openpilot-params` crate and its dependencies are unchanged. The uploader now
uses the adapter for `DongleId` and `AthenadRecentlyViewedRoutes`. Issue #61 can use
`get_string(&Params, key, &mut Logger) -> Result<Option<String>, Error>` and retain
its existing caller-level fallback policy.

The adapter validates registered key metadata before reading. Missing, empty and
unreadable files return `None`, matching the actual C++ `util::read_file` boundary.
Valid UTF-8, including embedded NUL, is retained. Invalid UTF-8 emits the original
WARNING text, Python bytes representation and `<ParamKeyType.STRING: 0>` label,
then returns `None`. The log record has the real Rust file, function and line.
Unknown keys remain errors; non-STRING keys are explicitly rejected by this API.
Logger transport errors propagate, including a closed socket. Registry defaults,
blocking reads and other typed conversions are outside this adapter's contract.

## Validation

`build_params_python.py` compiles unchanged `params_pyx.pyx`, `params.cc`, `util.cc`
and C++ logging with the actual PC headers, schema-generated headers and the
locked json11 dependency. The Python oracle isolates only Paths/hardware discovery;
it executes the real compiled getter, SwagLogger, formatter and ZMQ transport.
Its build provenance records source hashes, compiler invocations and module hash.

`check_params_string.py` compares 834 physical-file cases against the actual binding:
all 65 registered STRING keys without defaults, valid Unicode/NUL, every byte in an
invalid value, malformed UTF-8 forms, 500 deterministic byte sequences, unreadable
files, directories and unknown keys. Same-socket DEBUG barriers establish exact
warning order and absence. The closed-socket scenario checks both implementations
fail instead of silently supplying a fallback. Host and aarch64/QEMU runs pass.

`check_params_string_uploader.py` exercises missing/empty/invalid/valid identity and
empty/valid/invalid recently-viewed routes, using source and native uploaders with
both original and native log collectors (28 scenarios per binary architecture).
It verifies warning text/order, nonzero missing-identity exit, successful loopback
HTTP upload and uploaded xattr, cereal log publications and persisted disk records.
Both architectures pass. All files and signing keys are synthetic and temporary.

Nine native adapter/uploader tests and targeted Clippy pass. The generic aarch64
binary builds with Zig and executes under QEMU; this is not device acceptance.
No production daemon selection, workflow, raw Params dependency or user guide changes
are included. No vehicle, production credential or external upload was accessed.

Reproduction from the repository root (with Cython, pycapnp, pyzmq, requests, PyJWT,
cryptography and zstandard installed; original msgq Python binding on PYTHONPATH):

```sh
CARGO_INCREMENTAL=0 cargo build --manifest-path rust/Cargo.toml \
  -p openpilot-params-typed --example params_string_probe \
  -p openpilot-uploader --bins --locked
CARGO_INCREMENTAL=0 cargo build --manifest-path rust/Cargo.toml \
  -p openpilot-logmessaged --locked
python rust/tools/build_params_python.py --output "$EVIDENCE/original-binding"
PYTHONPATH=.:rust/tools python rust/tools/check_params_string.py \
  --binding "$EVIDENCE/original-binding/params_pyx.cpython-312-x86_64-linux-gnu.so" \
  --native rust/target/debug/examples/params_string_probe --output "$EVIDENCE/differential"
PYTHONPATH=.:rust/tools python rust/tools/check_params_string_uploader.py \
  --binding "$EVIDENCE/original-binding/params_pyx.cpython-312-x86_64-linux-gnu.so" \
  --binary rust/target/debug/openpilot-uploader \
  --collector rust/target/debug/openpilot-logmessaged --output "$EVIDENCE/uploader"
```

The binding builder accepts `--capnp-prefix` and `--zmq-include` for non-system
headers. The exact tested module suffix and absolute dependency paths are recorded
in the local ledger. Evidence: `.omo/evidence/params-string/evidence.json` in the
issue worktree. Parent integration owns cloud validation and the full-runtime gate;
#64 alone is not the first-device-test candidate.
