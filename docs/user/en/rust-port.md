# Experimental Rust port

This document covers the initial diagnostic tool and device source build work (#598).
The CPU diagnostic below does not replace vehicle processes or enable an additional AI model.

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
source builds without a complete runtime redownload take priority. `openpilot-runtime-build`
builds the required targets with a pinned native Rust 1.94.0 toolchain and offline Cargo dependencies.
Track progress and acceptance in [#598](https://github.com/bin9208/openpilot-rust/issues/598).

The current work branch selects source builds at ARM startup instead of automatically
downloading a complete runtime release for the checkout's commit. It uses the existing
Git install/update and restart flow. Pinned build inputs and default configuration are
included with the Git branch. First startup prepares the required tools and dependencies
automatically; later builds reuse the common cache. Check #598 above for the exact test
candidate commit and its results.
Normal startup starts the progress display and recovery port 6999 first, compiles changed
source using prepared dependencies and the existing Cargo cache, then starts manager.
Repeated full SDK/output audits, sealing and generation packages belong to the explicit
development path. An unchanged start skips preparation and compilation. On failure, the
error screen shows the log path and recovery stays available. First-time dependency
installation still requires the existing download/input preparation once. Policy and
unmeasured device results are recorded in the [#689 notes](../../naver/user-startup-689.md).

For device testing, install or update the candidate identified in #598 through the existing
Git flow, then restart. No manual Rust installation or separate input preparation command
is required. Check the commit in the software screen, normal display, recording and the
existing log upload path. For initial installation and a small change, record separate times
from the start of Git transfer until the display appears. Record device results in
[#528](https://github.com/bin9208/openpilot-rust/issues/528); CI timings do not replace device measurements.

The developer command `openpilot-runtime-assets --plan JSON --cache DIRECTORY` verifies
HTTPS assets by SHA-256 and byte count and stores them by content. Preparing the same
assets again transfers zero HTTP body bytes; changing one provider fetches only that
asset. `transferred_bytes` reports HTTP body bytes. This tool does not extract or install
a runtime. The build driver requires a clean Git checkout and reports the actual commit,
tree, recipe, compiler, linker and executable hashes. Initial and changed-source startup
with the actual inputs, and user acceptance on the device, are tracked separately.

Transient connection errors before any body arrives retry up to three times after
2, 4 and 8 seconds, within a total 1800-second transfer budget. Partial-body failures
and HTTP, size or hash errors remain failures; existing assets and the working runtime
generation are preserved.

For unpacked toolchains/providers, `openpilot-runtime-inputs --root INPUT_TREE
--output RECEIPT_JSON` records actual file hashes, lengths, modes and contained file
links. Bind this receipt to independently verified asset provenance, pin its SHA-256
in the recipe's `inputs`, and map the tree to the receipt in `input_trees`. The driver
in this explicit developer audit path checks the complete actual tree before and after building, rejecting changes, additions
and missing files. Capturing a new receipt alone does not establish trust; trees without
receipts are not reported as verified.
The [verification guide](../../naver/native-build-test-guide-598.md) records expected
results and the final device acceptance procedure in Korean.
