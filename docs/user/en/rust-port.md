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

Future development checks run on the designated `bin-actions` runner. ARM checks retain
their original commands in a separate QEMU/PRoot ARM environment and are distinct from
device validation. Development tools admit each operation by its estimated growth and
do not automatically delete logs or another task's caches. The device installer's space
and integrity checks remain in place.
