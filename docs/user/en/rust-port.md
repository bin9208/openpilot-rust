# Experimental Rust port

This independent repository has begun porting the openpilot device runtime to
Rust. The initial workspace contains scalar filters and a Linux CPU diagnostic.
It does not yet replace vehicle processes or enable an additional AI model.

On a Linux development host, with Rust 1.94.0 installed, run from `rust/`:

```sh
cargo run --release --locked --bin cpu-sample -- 1000
```

The optional interval is 1–60000 milliseconds (default 1000). Output is TSV.
100% means one logical core; values above 100% are valid for multi-threaded
processes. The processor column is the last observed main-thread core, not the
process's affinity or the placement of every thread. Unreadable/exited processes
and invalid counter deltas are omitted. Short intervals have coarse resolution.

This is a one-shot diagnostic, not a background service. It does not publish
vehicle commands. Host tests and generic aarch64 builds do not validate AGNOS
compatibility, CPU savings, temperature improvements, or safe vehicle operation.
Do not deploy the generic CI build to a vehicle; target validation remains open.

### Device source build preparation (#598)

Model preparation retains its existing Python path while performance improvements and
source builds without a complete runtime redownload take priority. The initial developer
tool `openpilot-runtime-build` builds selected targets with a prepared native Rust 1.94.0
toolchain and offline Cargo dependencies. It is not connected to automatic installation
or startup and does not yet provide a complete device source build.
Track progress and acceptance in [#598](https://github.com/bin9208/openpilot-rust/issues/598).

The developer command `openpilot-runtime-assets --plan JSON --cache DIRECTORY` verifies
HTTPS assets by SHA-256 and byte count and stores them by content. Preparing the same
assets again transfers zero HTTP body bytes; changing one provider fetches only that
asset. `transferred_bytes` reports HTTP body bytes. This tool does not extract or install
a runtime. The build driver requires a clean Git checkout and reports the actual commit,
tree, recipe, compiler, linker and executable hashes. Complete toolchain/ABI sealing and
automatic startup integration remain in development.

For unpacked toolchains/providers, `openpilot-runtime-inputs --root INPUT_TREE
--output RECEIPT_JSON` records actual file hashes, lengths, modes and contained file
links. Bind this receipt to independently verified asset provenance, pin its SHA-256
in the recipe's `inputs`, and map the tree to the receipt in `input_trees`. The driver
checks the complete actual tree before and after building, rejecting changes, additions
and missing files. Capturing a new receipt alone does not establish trust; trees without
receipts are not reported as verified.
The [verification guide](../../naver/native-build-test-guide-598.md) records expected
results and the final device acceptance procedure in Korean.
