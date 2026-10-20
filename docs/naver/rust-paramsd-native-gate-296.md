# paramsd native startup gate: first actual daemon consumer (#296)

The unchanged Python `check_paramsd_daemon.py` still owns the complete
continuous and restart comparison: 1,202 and eight input batches, exact
publication 1199 persistence, source event equality, and shutdown. Its
`StartupGate` passes two descriptors with `subprocess.Popen(pass_fds=...)`.
The Rust foundation already models its cursor and R/S tokens. This change
adds `StartupGate::attach`, which maps owned clones of the same ready-write
and release-read pipe ends into only the selected daemon child through the
repository's existing `command-fds` API. Both parent descriptors remain
CLOEXEC. The C hook still receives two distinct numeric FDs; daemon stdin,
stdout, and stderr remain unchanged. No process-wide inheritable FD state or
caller-written unsafe `pre_exec` is used.

The ignored cloud-only test
`native_mapped_gate_holds_actual_paramsd_second_poll_and_shutdown` is the
first real consumer. It starts the fresh `openpilot-paramsd` ELF in a private
Params/msgq namespace with a source-generated `CarParams`, a source-built
unchanged `paramsd_startup_phase.c` hook, and separately captured stdout and
stderr. It requires exactly one completed empty update before the held second
poll, then releases `S` and requires successful SIGTERM shutdown within five
seconds. Its ten-second ready deadline and the hook's existing 100 ms poll
remain unchanged. A process-group owner kills and reaps the daemon on failure.

The parent CI integration must provide absolute paths via
`PARAMSD_NATIVE_GATE_DAEMON`, `PARAMSD_NATIVE_GATE_LIBRARY`,
`PARAMSD_NATIVE_GATE_CAR_PARAMS`, and fresh `PARAMSD_NATIVE_GATE_EVIDENCE`.
Build the hook from unchanged `rust/tools/paramsd_startup_phase.c` using the
same `clang -std=c11 -O2 -shared -fPIC -Wall -Wextra -Werror ... -ldl`
command as `paramsd_startup_gate.py`. Generate `CarParams` with unchanged
`paramsd_fixture.car_params()` into an owned file. Seal these source inputs,
the resulting hook, CarParams bytes, and daemon ELF before and after the test;
retain `poll.jsonl`, stdout/stderr, and `result.json` in the estimation
artifact. Run the ignored exact test in the existing estimation job after its
source build and original five-case pytest gate, with a disk guard and
`--locked`:

```text
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools \
  --features paramsd-startup-gate-native --test paramsd_startup_gate --locked -- \
  --ignored --exact native_mapped_gate_holds_actual_paramsd_second_poll_and_shutdown --nocapture
```

This gate has **not** run in cloud. It proves actual native gate consumption
only after passing there. It does not establish the full daemon comparison or
retire `paramsd_startup_gate.py` or `check_paramsd_daemon.py`; the complete
same-source original/native pair and later absent-Python execution audit remain
required.
