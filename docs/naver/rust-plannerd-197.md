# Native planning owner — issue #197

The project-owned planning daemon, Carrot planner, longitudinal/lateral planners,
MPC policies and supporting policies are implemented in Rust. Local host and ARM
source comparisons, actual host IPC, native-boundary checks and target loader
checks have passed. Integration into the complete runtime, required exact-head
CI, post-merge checks and the user's eventual device comparison remain separate
gates. Keep [#1](https://github.com/bin9208/openpilot-rust/issues/1) and the relevant
acceptance work open; this component is not a first-device-test handoff.

Scope is tracked in [#197](https://github.com/bin9208/openpilot-rust/issues/197).
The work started at `340cc0b42ff7b214842b460289a013eda6fbcb70` and incorporates
IPC dependency `0697eff69843ffb02114d7ce3f8fdc058651ac5a`. Original source,
licensing and production process selection remain intact. No vehicle/C3X, NAS
or original-fork deployment was performed. No CPU or hardware benefit is claimed.

## Runtime and numerical boundary

`openpilot-plannerd` preserves the original serial construction and execution
order: CarParams, lane departure, longitudinal/lateral planners, fast-radar and
stopping-lead owners, publishers/subscribers, then Carrot initialization. It
retains core 4/FIFO51 on TICI, Hyundai liveTracks clock selection, the 25 ms
longitudinal interval, 100 ms liveTracks fallback, source validity checks, fast
overlay followed by stopping-lead conditioning, ordered publications and
runtimeTiming diagnostics. Navigation age is checked at the original Carrot and
post-MPC coasting points. Thresholds, following-time rules and radar selection
are unchanged. SIGINT stops the CarParams wait and polling loop. SIGTERM stops
the blocking CarParams wait cleanly; after CarParams arrives, the original
default SIGTERM disposition is restored.

The Rust owner includes LanePlanner2, lane departure, traffic-stop matching,
driving modes/follow gaps, confirmed lane-change credit, lead response/preview,
gap recovery/cutout/cut-in, turn limits and cruise coasting. Shared Rust
scalar/filter/IPC implementations are reused where their source contracts match.

The approved M4 exception retains the original native acados solver. Python,
Cython, NumPy and CasADi run only in generation and source-oracle tooling. Runtime
policy does not call them. The native runtime payload is `manifest.json`, both
generated solver shared libraries, the pinned acados/HPIPM/BLASFEO/qpOASES shared
libraries, and their licenses/provenance. The default directory is
`plannerd-acados` beside the executable; `PLANNER_ACADOS` or `--solver` supplies an
explicit artifact. The builder's SDK/generated-source directories are build
evidence, not additional runtime interpreters.

The Rust boundary owns native capsules and library lifetimes, uses typed fields,
checks stage/buffer dimensions against generated contracts and native queries,
and validates every artifact library against the manifest before loading it.
The original native solver and its external libraries remain explicit native
dependencies, not newly implemented Rust numerical solvers.

## Pinned artifact and source provenance

`uv.lock` pins acados `0.2.2.post103`, bundled CasADi `3.6.7`, NumPy `2.5.3`,
pycapnp `2.1.0`, and Cython `3.3.0`. Local original Python is `3.12.14`.

| Artifact | SHA-256 |
| --- | --- |
| Host acados wheel | `3b451852e83d62815cead999ab31073db9be60307f772650336cdd4534f12b9e` |
| Tracked ARM acados wheel | `2ad9fcebef1f65112a9cebe8a093977310602e1325d442dc345beaf512679211` |

`build_plannerd_acados.py` executes the original generator files and verifies all
304 installed acados/CasADi generator files against the locked host wheel. It
uses headers and libraries from the target wheel, records compiler identity,
commands, generated sources, headers and ELF machine identities, and preserves
the original GNU11/O2/PIC contract. ARM compilation additionally uses the source
Clang/Cortex-A57/TICI contract. Host Haswell headers are never mixed with ARM
Cortex-A57 libraries. `build_plannerd_source.py` builds the unchanged original
Cython interface against the verified artifact; cross compilation requires
explicit target Python and NumPy headers.

Upstream acados, BLASFEO, HPIPM and qpOASES license texts, revision-pinned URLs
and hashes are in `rust/crates/plannerd/licenses/` and copied into each artifact.
The comma dependencies recipe identifies the upstream license revisions; exact
locked wheel hashes identify the actual binaries. These are distinct evidence.
The CPython compensated-window-sum provenance and full license are also retained
in the crate. Generated C preserves its original notices.
The qpOASES license is kept byte-for-byte, including upstream whitespace; only
that license file is exempted from Git's whitespace diagnostics.

## Local verification ledger

Evidence is retained under the owned worktree's
`.analysis/scratch/2026-10-04-plannerd/`. Frozen host/ARM executables and hashed
source archives remain available after shared reproducible build-cache cleanup.
Source comparisons below use exact floating-point bits, with no numerical tolerance.

| Check | Result and evidence |
| --- | --- |
| Host package and lint | 28 tests; binary, examples and strict Clippy pass (`package-11`, `target-clippy-1`; `frozen-4`, `runtime-frozen-5`, `gap-frozen-1`) |
| Pure Rust Miri | 27 tests at each of default, strict provenance, symbolic alignment/preemption and Tree Borrows (`miri-1`, pinned nightly 2026-09-29) |
| Original policy helpers | 17 lane-departure, 322 coasting, 15 lead-acceleration and 30 path-geometry cases (`policies-frozen-1`) |
| Original fast/stopping radar tests | 39 original pytest tests, 311 ordered actions from 36 owners; all lead fields/roles/masks/reasons exact (`radar-source-2`) |
| Confirmed lane-change credit | 42 original pytest tests, 857 actions from 18 owners; includes 187 positive-confidence plans and 9 positive-credit calls (`gap-source-3`) |
| Complete policy owners | 360 successive Carrot/longitudinal/lateral/MPC states, matrices, outputs and Params operations exact (`owner-source-4`, rechecked by `sanitizer-owner-1`) |
| Original complete host main loop | 712 polls and 1,080 ordered cereal publications exact, including source-computed triggers and Params ledger (`runtime-source-3`, `runtime-final-host-1`) |
| Alternative trigger policies | VW and stock-longitudinal variants each pass 712 polls/1,080 publications; VW uses zero liveTracks triggers (`runtime-source-vw-1`, `runtime-source-stock-1`) |
| Actual original C++ IPC | Original Python and Rust daemons each run twice with reused per-owner prefix. Each run has 121 ordered publications and valid steady-state messages on all three services (`ipc-3`) |
| Actual solver faults and recovery | Typed cereal NaNs cause lateral solver failure and three longitudinal status-4 resets; 160 polls/240 publications and warning behavior match (`runtime-source-fault-4`) |
| Generated-C sanitizers | ASan/UBSan: 12 ABI solves, 360 complete owner states, and the 160-poll fault/reset loop pass (`sanitizer-native-1`, `sanitizer-owner-1`, `sanitizer-fault-1`) |
| Rejected native boundaries | Wrong manifest architecture/hash, missing library, wrong buffer size, terminal control and oversized stage reject cleanly (`boundary-rejections-2`) |
| Startup lifecycle | SIGINT/SIGTERM exit cleanly while awaiting CarParams; invalid frame limit rejects (`startup-signals-1`) |
| Parent lifecycle review | 13 real-source/native cases cover missing, empty and unreadable CarParams, directory recovery, four unreadable settings, both signal phases and fatal conversions. The original Rust failures are retained before the fix (`plannerd-review-197/lifecycle-red-v1`, `lifecycle-green-v1`) |
| Parent follow-up build and IPC | 29 package tests, all targets and strict Clippy pass. Four actual IPC runs each publish 121 messages with exact policy payloads and default SIGTERM termination (`plannerd-review-197/params-signal-*`, `ipc-green-v1`) |
| ARM build and original ABI | GNU aarch64 binary/six examples build; original ARM Cython and Rust agree on all bits in 12 solver iterations (`arm-frozen-1`, `arm-source-1`, `arm-native-1`) |
| AGNOS loader and closure | Actual published AGNOS 19.8 loader/libraries execute the binary and native solver; all 12 ELF dependency/version closures resolve (`agnos-runtime-1`, `elf-closure-2`) |
| Complete original ARM main loop | Original ARM Python/NumPy/pycapnp/Cython main loop versus Rust under the AGNOS loader: 712 polls/1,080 publications and the 160-poll/240-publication fault/reset case are exact (`arm-main-compare-3`, `arm-main-compare-2`) |

Final reproducible artifacts are `artifact-host-4` and `artifact-arm-3`; both
original Cython bindings were rebuilt with `build_plannerd_source.py`. The final
formatted tools pass pinned Ruff 0.16.7 and syntax checks. `final-qa/commands.json`
records nine passing checks after that cleanup: ABI, policy helpers, radar,
confirmed lane gaps, complete owners, complete main, solver faults, radar-off
mode and actual IPC. The native code is unchanged by the tool-formatting pass.

The whole-owner scenarios include selected radar leads, traffic stop/restart,
lane-change inputs, experimental mode, force deceleration, gas/brake/reset,
invalid radar, stale pose and parameter reloads. The actual IPC runs exercise
CarParams waiting, liveTracks-only startup, model/liveTracks switching,
experimental mode, stop/departure, restart and termination. Their longitudinal
publications include 35 liveTracks and six model triggers per run.

The parent follow-up evidence is in the integration worktree's
`.omo/evidence/plannerd-review-197/`. Its frozen host daemon SHA-256 is
`7335aa86781177f05c4372afe02c8dedcbf26088b3e1da2682f75312f9043c97`.
Params filesystem read errors now use the source empty-value defaults and allow
CarParams to recover. Unknown keys and invalid numeric values remain errors.
The raw Python harness exits with KeyboardInterrupt, whereas the original
manager launcher and Rust exit cleanly on SIGINT. Invalid numeric conversions
abort the C++ source binding; Rust reports a typed error and exits with status 1.
These exit boundaries are recorded explicitly in the lifecycle receipt.

The corrected ARM daemon SHA-256 is
`f7161696baa2be17ac115f7db88a22eb40aca844b5ed14d191e80d64e522cea8`.
Its all-target build, Params regression under the extracted AGNOS loader and
12-file dependency/version closure pass (`params-signal-arm-build-v1`,
`arm-params-test-v1`, `arm-daemon-loader-v1`, `arm-elf-closure-v1.json`).

Whole-main deterministic comparisons exclude only the two plans'
`solverExecutionTime`. Actual IPC compares complete semantic payloads and
validity, excluding `processingDelay`, `solverExecutionTime`,
`plannerExecutionTime` and `fastRadarExecutionTime`, which measure separate
process executions. No policy or validity fields are omitted.

ARM was compared to its actual original source execution. An initial comparison
against the x86 source found two small Float32 differences in jerk/curvature
rate. Both match the ARM original exactly; no tolerance or rounded comparison
was introduced. The original pycapnp emits its existing QEMU filesystem warning;
the warning and all failed harness attempts remain in the evidence.

Miri covers pure Rust, not foreign numerical libraries. Its artificial extra
rounding errors are disabled for the exact-arithmetic assertions; this changes
no native/source comparison. ASan/UBSan instruments
generated solver C and checks allocator lifecycles; the pinned prebuilt acados,
HPIPM, BLASFEO and qpOASES implementations are not themselves instrumented.
QEMU/source comparisons do not establish device scheduling, sensor timing,
loaded driving, CPU savings or whole-runtime integration.

## Reproducible CI inputs

Run from the repository root with the locked source dependencies, a host acados
wheel from `uv.lock`, Rust 1.94.0, and the standard original Params/msgq Python
bindings. Preserve at least 25 GiB free plus each operation's growth; use bounded
jobs and disabled incremental compilation. The builder and source-builder
commands enforce their own native-build disk checks.

With `PLANNER_PYTHON`, `PLANNER_GENERATOR`, `PLANNER_HOST_WHEEL`, and
`PLANNER_WORK` set to the prepared interpreter, installed acados package, locked
wheel and owned output directory:

```sh
python rust/tools/build_plannerd_acados.py --architecture x86_64 \
  --python "$PLANNER_PYTHON" --acados "$PLANNER_GENERATOR" \
  --wheel "$PLANNER_HOST_WHEEL" --output "$PLANNER_WORK/runtime"
python rust/tools/build_plannerd_source.py --artifact "$PLANNER_WORK/runtime" \
  --python "$PLANNER_PYTHON" --acados "$PLANNER_GENERATOR" \
  --generator-wheel "$PLANNER_HOST_WHEEL" --output "$PLANNER_WORK/source"
cargo build --manifest-path rust/Cargo.toml -p openpilot-plannerd --bins --examples --locked
"$PLANNER_PYTHON" rust/tools/plannerd_runtime_source.py \
  --source-native "$PLANNER_WORK/source" --artifact "$PLANNER_WORK/runtime" \
  --binary rust/target/debug/examples/plannerd_runtime_trace --output "$PLANNER_WORK/main"
```

The same runtime command supports `--faults --frames 80`, `--brand volkswagen`,
`--stock-longitudinal`, and `--source-only` for an original ARM source run followed
by a separately launched QEMU Rust comparison. Additional checks use
`plannerd_acados_source.py`, `plannerd_policy_source.py`,
`plannerd_owner_source.py`, `plannerd_radar_source.py`, `plannerd_gap_source.py`
and `plannerd_ipc.py`. For actual IPC, put the original C++ msgq import root first
in `PYTHONPATH`, use Python `-P`, and supply the original Params module with
`--binding`. Source-oracle Cython must be the lockfile's 3.3.0.

`rust.yml` now requires `planner-runtime` on both x86-64 and native ARM runners,
plus `planner-memory` at all four Miri modes. The runtime job installs the locked
native wheel, verifies its generator files, builds both original Cython solvers,
and runs `check_plannerd_ci.py`: ABI, policy helpers, radar, lane gaps, complete
owners/main, solver faults, VW, stock longitudinal, radar-off, actual IPC and
lifecycle. It retains the generated native payload and original-source evidence.
The host job also uses `check_plannerd_native_memory.py` for instrumented
generated-C ABI, complete-owner and fault-loop checks. Existing fast,
integration, mapped-doc and aarch64 requirements remain required.

The new 12-lane CI recipe passes locally on the frozen corrected host binaries
(`plannerd-review-197/full-ci-host-v1`), as does its three-lane generated-C
sanitizer recipe (`native-memory-ci-v1`). A second full pass uses freshly built
original Params and C++ IPC bindings with the CI-pinned Cython 3.3.0
(`ci-cython-params-v2`, `ci-cython-msgq-v2`, `full-ci-host-v2`). Local first attempts
lacked staged include paths and setuptools; those failures remain recorded and
the successful commands reuse existing dependencies without source changes.
CI helper regressions execute a failing
comparison and a failing Miri command to require nonzero job results and stop
later checks. Hosted exact-SHA Actions and post-merge success are not claimed by
this local ledger. Complete normal startup
and the existing log-upload path remain mandatory before the user's first
device comparison. Public user guides are unchanged because this component
preserves policy and adds no user setting or selected production daemon.
