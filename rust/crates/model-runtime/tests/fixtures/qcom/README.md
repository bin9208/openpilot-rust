# QCOMCL oracle fixtures

These kernels were authored for this port and compiled for a630 using the
AGNOS `libllvm-qcom.so` at commit
`ec1cf237a84565a056dbbc6f433b1b1c20c07a2c`, path
`userspace/root/usr/lib/aarch64-linux-gnu/libllvm-qcom.so` in commaai/agnos-builder.
The compiler SHA-256 is recorded in `provenance.json`. The compiler itself is
not redistributed here.

`rust/tools/qcom_fixtures.py` runs the original `QCOMProgram`, `QCOMArgsState`
and `QCOMComputeQueue` from this repository's tinygrad snapshot, as integrated
in cc0ca7f0943bbc1cf1c1f0ec1ea82bc431b8e672. It uses Python-owned host allocations
and fixed GPU addresses; it never opens a GPU device. The compiler runs under
ARM64 Python with QEMU on the development host. The `.args` files contain the
full original argument allocation, including zero padding. JSON contains all
original metadata, dispatch words and a separate memory barrier.

Rebuild into a new directory with ARM64 Python and the pinned compiler:

```sh
PYTHONPATH="$PWD:$PWD/tinygrad_repo" LLVM_QCOM_PATH=/path/to/libllvm-qcom.so \
  python rust/tools/qcom_fixtures.py /path/to/new-fixtures
```

Rust parser/argument/packet code is derived from the same MIT-licensed tinygrad
implementation and its bundled Mesa register definitions. Its copyright and
permission notice is retained in `src/qcom/LICENSE`.

Fixture equality establishes host serialization parity, not GPU execution or
numerical correctness. The buffers contain synthetic addresses only.

`python rust/tools/qcom_fixtures.py --verify rust/crates/model-runtime/tests/fixtures/qcom`
recomputes metadata, argument bytes, integer/fractional command streams and KGSL
ABI from the original Python implementation on a host. This mode uses the checked-in
compiled binaries and does not load the ARM compiler or open a GPU.
