Independent expectations from unchanged `check_proclog_reference.py::reference`
and `make_process`, calling the original Python collector directly over mapped
synthetic procfs. No native collector, encoder or candidate was imported or run.
`manifest.json` pins the checker, runtime source, service config and full schemas.

`original.json.gz` contains 176 complete Event dictionaries and original capnp
packets: two smaps modes, 22 cycles, and four clock/page-size pairs. These include
native wire float32 rounding, signed child CPU time, invalid cmdline UTF-8,
process-name parentheses, RSS PSS gating, and the cycle-20 cache refresh.
The original host was 100 ticks/second and 4096-byte pages. Other parameter sets
are (1,1024), (250,8192), and (1000,65536). They are source-only controls, not
claims of running a candidate on those hosts.

`procfs.json` preserves every initial synthetic fixture file byte and symlink
for both modes. `queue-int.json` captures CPython int(bytes) acceptance/rejection
for the candidate queue response. Tests check immutable hashes before use.
Full schema reflection and mutations of all 70 scalar/text leaves prevent
selected-field comparison from silently dropping any original Event/procLog data.

`protocol.json` captures 28 executions of the unchanged checker with controlled
original-wire child responses (14 conditions in each smaps mode). It preserves
queue failure ordering, size bounds, short/truncated reads, cache timing, child
exit errors and the original ten-second final wait. Notably, an extra trailing
frame is accepted by the original; the native checker must not silently tighten
that contract. Live native peer tests remain required in addition to captures.

`process-wire.json.gz` contains 88 additional wire-only original frames for
100 ticks/second with 4/8/16/64-KiB pages. The original source is invoked by the
same AST/mapped-procfs capture, never by a native oracle. Linux process controls
select the captured host pair and replay only these packets through an owned
shell peer. Protocol/mutation/cancellation tests exercise real CLI processes;
source/executable changes and FIFOs use unique private source copies.

The native controller's full 44-cycle execution matched the unchanged original
before and after retiring check_proclog_reference.py together with its sole
Python caller check_aarch64.py. Native execution used an empty PATH and no
Python process. Fresh post-deletion builds and all nine procLog tests passed,
including real child cancellation, source/executable identity and FIFO controls.
Complete Event fields remain compared; no floating tolerance was introduced.
The fixture decoder preserves the original captured binary64 values exactly.
Shared device_probe.py and the separate procLog daemon checker remain live.
