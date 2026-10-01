# Native platform runtime integration (#116)

This combines manager #106, hardwared #111 and serial modem #113 on the startup
and hardware prerequisites in #94/#103. The source implementations and production
process selection remain unchanged. Full runtime conversion is tracked in #1.

The required `rust platform runtime` Actions job builds the native executables,
examples and original IPC/Params bindings, then executes these existing gates:

- Manager source initialization and transition scenarios, actual lifecycle log
  collector, supported-car export, live managerState/owned child lifecycle and
  native boot/registration/hardware exit adapter composition.
- Modem source AT/state comparisons through owned PTYs, continuous native
  reconnect/shutdown, descriptor closure and exec-error/startup boundaries.
- Hardwared source fan/power/thermal policy comparisons and continuous native
  deviceState/Params/logging/stats observations through isolated native msgq.

The aggregate `rust checks` requires this job together with all existing jobs.
Host workspace fmt/clippy/tests/release and generic aarch64 compilation remain
required; platform tests run concurrently with the existing runtime jobs. The CI
policy test rejects failure, cancellation, skipping or absence of any dependency.
Local validation reuses the focused component evidence and checks merge metadata
and CI policy; repeatable complete builds run in Actions.

The first integration Actions platform job passed. Its separate startup catalog
gate caught missing candidate-availability entries for hardwared and modem; the
catalog now records both native packages while leaving production selection
unchanged. The existing inventory comparison remains required.

See [manager](../naver/rust-manager-106.md),
[hardwared](hardwared-validation.md) and [modem](modem-validation.md) for exact
source scope, host evidence and native external dependencies. These are continuous
host candidates. Complete daemon binding, startup UI, remaining project-owned
services, AGNOS packaging and normal startup/upload comparison are still pending.
No device access, board action, production runtime selection or CPU benefit has
been claimed. The first user device comparison remains gated on the full runtime.

Docs-Not-Needed: isolated native runtime integration; no setting or selected user
workflow changes.
