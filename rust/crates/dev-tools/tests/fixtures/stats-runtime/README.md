# Statistics production runtime QA source contracts

`source-pins.json` records the unchanged runtime/flood checkers, their shared
original logging adapters/schema inputs, and the production entrypoint and flood
producer. All 17 inspected bodies match CI commit
`2106ad4791d488a6361f93b85a8b43eb3d2de1fe`, tree
`0416b71d5b6bd76891ca58adeb144180cb3f2c6c`.

The new validator tests use synthetic observations derived from the original
assertions. They are not captured native candidate answers. The process test
requires a fresh source-sealed production `statsd-rs` and `stats_flood`; the
existing `stats_daemon` example is not an equivalent startup substitute.

Fresh, unchanged original checkers passed the one runtime and two flood cases
against a newly built and frozen production pair at `142721d8`. The build and
capture seal covers 373 local Cargo/source and direct original/native-schema
inputs, plus both candidate ELF hashes. Existing original msgq source copies
were checked against current inputs before reusing the readonly binding.

`original-cli.json` retains 16 actual original arity/help/output effects, with
only the controlled repository, bundle, and interpreter paths normalized. In
particular, runtime rejects an existing output root; flood accepts an existing
root when the per-signal children are new and preserves unrelated root files.
The first capture harness wrongly expected flood to reject that root; its
failure log and raw successful outputs were preserved, and a corrected capture
used a new unique directory. No original checker source was changed.

Genuine missing-API RED preceded implementation. The native gate now passes
nine runtime/flood tests and all 15 predecessor tests, including real production
processes, 16 original CLI controls, owned cancellation and foreign socket
preservation. Strict Clippy and scoped formatting pass. A separate actual RED
confirmed the retirement source prerequisite: only these two checker paths may
be absent; every present input remains hash-checked. Missing retained inputs,
changed bytes, dangling symlinks and directories are rejected. Absent paths are
checked again through final publication.

The predecessor cancellation fixture exposed a transient pre-exec environment
observation in this gate (the #396 test-race family). Its same reserved pidfd
must now remain alive until the inherited private token is visible, within the
original 15-second startup deadline. Token identity, two-second cancellation,
owned cleanup and replacement-endpoint assertions remain unchanged. This is a
fixture acknowledgement repair, not a statistics runtime defect.

Fresh native checker and exact executed test ELFs were frozen with the unchanged
production pair. The before-deletion native runtime/flood proof passed all three
cases with strict stable-report comparison to the original observations and
15 fully audited execve/execveat attempts, zero Python. Dynamic clock values,
started selection, CPU ticks above the original minimum and measured shutdown
elapsed time are retained raw and independently validated; they are not fixed
expected answers. The 373-input production seal and scoped authored tool seal
do not claim to reconstruct the original binding's historical compiler run or
the entire Python import dependency graph.

After reviewed native caller migration and the successful before phase, only
`check_stats_runtime.py` and `check_stats_flood.py` were removed. The same frozen
ELFs then passed all three after-deletion cases with 15 audited exec attempts,
zero Python. Normalized reports match both original observations and the before
phase; all four input hash maps lost exactly those two historical source keys,
and retained source/executable hashes are unchanged. No original was restored
or executed after deletion. Fresh post-deletion tests, Clippy and builds are now GitHub Actions–owned under the user workflow override; no additional local execution is authorized. Existing completed local results above are retained without claiming that pending CI passed.
`CAPTURE-HANDOFF.json`, original and RED logs, frozen raw observations, and
source/ELF seals are in
`.analysis/scratch/2026-10-19-stats-runtime-tools/`.

A subsequent CI collision fixture repair explicitly sets the preexisting input
file to the captured 0664 mode, avoiding runner-umask dependence. The completed
24-test gate and frozen receipts above predate that fixture-only source delta;
they are retained historical results, not fresh validation of the updated file.
The updated snapshot and all remaining fresh validation are exact-SHA CI-owned.
