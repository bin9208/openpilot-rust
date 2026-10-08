# Rust Carrot Navi conversion (#206)

Issue: https://github.com/bin9208/openpilot-rust/issues/206

The candidate ports the original HTTP/WebSocket navigation receiver, session
and payload policies, Cereal publications, discovery and shutdown behavior.
It preserves the original `--host`, `--port`, `--no-beacon` and `--no-cereal`
options. Required boundaries are Linux sockets/threads/clocks, Params and
exclusive carrotNavi/carrotNaviMedia msgq endpoints. Production selection
remains unchanged.

The independently completed worker retained 179 source cases/612 steps, 58 JSON
cases and healthy wire examples. Final native v7 matches 56 actual socket and
Cereal observations, including all 27 enabled streams and complete image/render
bytes. The 21-observation lifecycle lane covers heartbeat, coalescing, CLEAR,
Params bootstrap, map-change closure, new sessions and stale-session rejection.
Its unchanged process logic reuses v6 lifecycle evidence; only actual wall-clock
millisecond conversion changed afterward. A focused integer-boundary regression
and final v7 socket comparison cover that correction.

Discovery/retry ran in an isolated namespace containing only the owned loopback
address: three exact beacon packets, occupied-port retries/recovery, SIGTERM and
ten-retry exhaustion. No vehicle, NAS or external LAN endpoint was used.
Final worker executable SHA-256:
`28f85979b3f5ffff23924d5b141d99d7fc1f3010433ac77f6ae4a741aaff5143`.
Detailed preserved identities, raw traces and limits are in the issue worktree's
`.omo/evidence/carrot-navi-206/RESUME_HANDOFF.md` and `resume-native-v7/`.

Build with `cargo build -p openpilot-carrot-navi --features native --locked`.
The existing `carrot_navi_runtime.py` accepts `--native-bin` or
`--source-python`, the original `--binding`, `--pythonpath`, `--output`, and
`--cereal`/`--lifecycle`. Existing `carrot_navi_qa.startup` provides the owned
namespace and exhaustion lanes. The source oracle uses its recorded Python
3.12.14, aiohttp 3.13.3, pycapnp 2.2.4, Cython 3.2.4, setuptools 82.0.1,
psutil 7.2.2 and zstandard 0.25.0 environment.

The integration carries the existing strict comparisons into host Actions and
adds a native ARM build using the same original package. Only generated session
IDs, dynamic bound ports, positive actual clocks and asynchronous publication
counts are normalized; raw captures remain available. No general numeric
tolerance is added. A local integration rebuild is deferred under the disk
guard. Exact-head hosted results, whole normal startup/log upload and device
acceptance remain outstanding. This is not a CPU or vehicle-validation claim.

## Integration preparation, 2026-10-08

This candidate is composed with the pending radard integration at `2cd8a1bd`,
which includes merged Navd dev `740ade11`. CarrotMan, RadarCAN, Navd and radard
requirements remain enabled alongside both Carrot Navi jobs. The union merge
preserves both CarrotMan and Carrot Navi packages in the workspace lock file.
Local connection checks pass 16 workflow-isolation tests, 10 Card/RadarCAN
CI-policy tests, locked offline metadata and diff whitespace checks. These
checks do not replace the independent source receipts or the pending hosted
host/ARM and normal-startup/upload gates. No local corpus rebuild was performed.

Radard PR 229 subsequently passed all required checks at
`de3b73dc682e0f4171c3bf849f6610d25a86bf98`, including both native Radard
architectures, Card, workspace and aarch64. Rust run `37727864726` and
integration run `37727864956` passed. It merged normally into dev as
`abde77f51419d27e24dfdce58253085e11dbeaf5`, which is now composed into this
candidate. The original Carrot Navi implementation and comparison policies
remain unchanged. Post-merge Radard checks and this candidate's new hosted
checks remain separate from those successful pre-merge results.

The first native ARM job passed in run `37732559164`. Its host build, tests,
Clippy and policy execution completed, but the first original process exited
before health because the CI dependency list omitted `pyzmq`. Original Params
loads swaglog and hardware modules, which also import NumPy, requests and
pyserial. These four packages are now pinned to the existing successful local
oracle versions (`27.2.0`, `2.5.3`, `2.34.2`, `3.5`), and CI imports the actual
Params binding and receiver before the native build. That complete import
passes locally using the cached original binding and environment; no dependency
was installed or full source comparison repeated locally.

Failed host job `113164776643` and artifact `11530763214` retain the import
trace and executable. The same run separately encountered the managed-entry
collector fixture hazard tracked in issue 184. Its preserved correction is
included without changing runtime behavior or comparison timeouts; see
`rust-managed-entry-readiness-184.md`. Required checks will run on the updated
composition before any merge.
