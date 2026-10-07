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
