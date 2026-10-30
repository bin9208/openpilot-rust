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

The `dev-md` native Jetlink candidate binds each App session to the model selected
offroad. Select **External GPU model (1)** to use its actual output once the live
session/artifact contract matches and the vehicle is stopped with all control
disengaged. The Shadow mode and its activation edge are removed; old saved values
1 and 2 both request external output. Off (0) remains the default and is restored
on manager restart. Historical device acceptance is diagnostic only: `validated`
remains false without matching evidence. Live frame/deadline checks and the loss
latch remain enforced; after loss, acknowledge Off while stopped/disengaged before
selecting external again. Host tests and ARM emulation do not establish device
or driving acceptance. See the
[Jetlink settings](settings.md#jetlink-external-model-experimental).

For a compatible host, the candidate forwards saved Wi-Fi profiles over the
private Jetlink connection and waits for host preparation before inference.
Wi-Fi passwords are excluded from status files and logs. A legacy host can
request the pinned model; its download is verified before transfer, and
preparation stops if the device goes onroad or the cable disconnects.
