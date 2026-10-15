# #338 native local radar replay and packaging

Goal: exercise the dev-md native radar candidate from local logs and source-pinned fixtures, with an identity that cannot silently reuse the stable Python replay cache. Source behavior is pinned to `e8e152dbcfd9574a1791ed12026d1880e87689b3`; dev stays `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. No NAS publication or device deployment.

## Design and acceptance

The replay CLI will reuse `openpilot-radard` controller/wire policies and `openpilot-radarcan` decoder/Engine interfaces. Recorded liveTracks are an explicit input mode; raw CAN decoding is a separate mode and preserves radarInput CAN/ego batch joins. Recorded radar-track inversion must not be applied twice. Explicit normal/flipped alternatives are analysis choices, not saved vehicle settings. Model velocity, lane lines/probabilities/deviations and lane-change state must survive decoding.

Read full Cereal schema from raw/zstd/bzip2 route streams; use recorded event times, model timestampEof and current radar timing policies. Output is offline analysis data with input validity/freshness and selected leads, never vehicle actuation. JSON prepared-frame input supports the exact final-source fixtures without involving a Python runtime provider.

The native packaging command will stage only the native replay executable, required DBC assets and source/license/identity metadata. Embed upstream pin, protocol version, source/schema fingerprint in the binary; compare the embedded identity against the selected checkout before packaging. Derive cache identity from that source identity, binary hash and input/options identity. Refuse stale executable/source combinations. Source revision and dirty source fingerprint are distinct fields: a Git SHA alone cannot identify this uncommitted candidate.

## Verification sequence

1. Add failing regressions for retained stopping/lane metadata, recorded inversion preservation and explicit analysis alternatives, raw CAN/ego joins, source/cache identity and stale binary rejection.
2. Implement the native replay adapter and CLI with existing native interfaces; compare complete outputs to pinned source fixture results.
3. Implement native local bundle creation and inspect its contents/identity; ensure no retained Python executable/provider is required.
4. Run host tests and local packaged replay; cross-link ARM and use available QEMU for the same meaningful cases. Keep numerical host-vs-ARM/reference limitations explicit.
5. Coordinate Bosch/ARS interface changes with vehicle/radarcan owner; the package must carry actual updated decoder assets and invalidate old cache identity.

## Current state

Native replay, DBC generation, host/ARM tests and caller migration are implemented below; the final source-matched bundle smoke awaits the parent-coordinated integration freeze. The existing `build_bundle.py` is deliberately not used as native evidence: it exports committed Python/Cereal/DBC source and has no native replay executable path.

## Native adapter implementation and first verification

`openpilot-radard` now owns `src/replay/` and the standalone `openpilot-radar-replay` binary. The prepared JSON-lines interface consumes production controller inputs, explicit options and an explicit recorded-inversion flag; it has no source-test predictor/reset injection. `recorded` streams full-schema raw/zstd/bzip2 logs and retains original lead data alongside recomputed outputs. Shared `daemon::input` preserves the exact daemon decoder/time policy. Missing inputs are reported, and native freshness/validity results accompany output. `raw` connects the existing radarcan Engine/Io and CAN/ego decoders with explicit offline parameters and numerical/DBC paths, preserving batch metadata rather than synthesizing latest-ego joins.

Build-time identity hashes dirty native source, schemas, DBCs and pinned upstream metadata with deterministic names/content boundaries. Prepared/raw receipts contain source/input/options/binary and actual external-asset hashes as applicable. Exclusive local bundle creation rejects an executable whose embedded source identity differs from the selected checkout. This stale-source guard was exercised successfully after subsequent shared edits. The bundle supports prepared/recorded replay and optional explicitly verified raw-CAN numerical/DBC providers. Its final successful source-matched package smoke remains pending.

Host planner/radar test batch and all-targets clippy pass. New tests verify full-schema stopping/lane inputs, missing-topic handling, identical raw/zstd/bzip2 replay, and differential inversion affecting only frontRadar. The two full-schema replay integration tests also pass under ARM QEMU. `rust/tools/check_native_radar_replay.py` compares the actual production CLI against exact final-source outputs: **6,696 frames across 327 source owners, zero differences including float bits**. Source cases requiring test-only predictor/reset injection are explicitly excluded from this production-interface fixture; they remain covered by the separate complete controller reference gate.

Final native bundle smoke, complete source replay-adapter policy comparison and Bosch/ARS decoder integration remain pending. Source generation, raw-CAN regression and content-bound cache-key tests are verified below. This is local candidate tooling, not a stable NAS replay-service deployment or device evidence.

### DBC tool conversion in progress

`rust/upstream/dbc-generators.json` records the actual final `e8e152db` Git blobs/hashes for all selected DBC inputs, the 45 complete generated output hashes, and nine declarative fixed radar-schema overlays required beyond the tracked source tables. The native compositor implements the original one-level IMPORT order and exact output text; static radar schema constructors become declarative native schema data. Retained original script hashes are checked when those provenance files exist; Python source/execution is not required by the native implementation. Unknown/changed selected source inputs and output-hash drift fail explicitly.

The vehicle owner applied the two final Hyundai BLINKER_STALKS TAP declarations; all local DBC inputs now match the final source fixture. Native generation and CLI pass on host and ARM: 103 assets match the actual pinned source/raw output hashes, including all 45 compositions. Raw-CAN regression preserves the first-sweep initialization, exact CAN/ego metadata joins, invalidity and explicit missing-metadata error. All five library tests plus all four integration tests pass under ARM/QEMU. The ARM NumPy provider needs system libz through libgfortran; the minimal cross SDK lacked it, and the repeat used only cached AGNOS 19.8 libz SHA256 `e3bc90ee8cce46778c2f46c2a3e580d3b930f7b455910f25ea400a0b797fb9dc`. This is an external target OS dependency, not hidden source or device evidence.


### Candidate pipeline migration

Candidate RadarCAN host/ARM and Card fixture CI now build/use the native DBC CLI. Three native asset caller tests and 24 workflow-isolation tests (367 subtests) pass. The project-owned `card_prepare_assets.py` and `radarcan_prepare_assets.py` were removed after their callers migrated and passed. The remaining Card source-oracle adapter invokes the native executable; it performs no Python schema generation. Controls staging migrates to native `stage-dbcs`, whose final compile/caller smoke is pending the next coordinated slot; its previous Python script is retained until that check passes.

The Rust candidate CI has a source-matched local bundle step with no publisher. Stable NAS publication/workflow remains untouched. A bundle contains the native executable, source/license metadata, canonical DBCs and optional verified numerical libraries; it refuses changed source identities and records source revision separately from dirty source fingerprint. Final package verification is deliberately deferred while other owners edit the shared checkout.

The native DBC caller tests also establish that generation leaves selected source tables unchanged. Two checked-in corner DBCs at the final source pin omit only the three-line composition header that the original generator adds; the canonical package intentionally uses original-generator bytes, while `stage-dbcs` preserves existing source tables as the preceding test staging did. Native Card/RadarCAN canonical assets share the same 103-table byte map.

Native receipt identity includes immutable source-pin fixtures, native C/Rust/shader sources, schemas and DBCs. Central coverage/evidence inventory is excluded: recording a binary receipt there must not invalidate that same binary recursively. The executable hash and actual external-asset hashes separately bind compiled code/flags and providers. Recorded original leads now include their own event timestamp alongside model time and radar-to-model skew; they are not presented as an automatically aligned ground-truth lead. Automatic late-metadata source-policy discovery and complete legacy upload/export record compatibility remain unclosed adapter work; current CLI callers pass explicit source-selected options/configuration.

2026-10-15 bounded staging closure: native `stage-dbcs` CLI smoke added the 38 missing tables, repeated with zero additions, and preserved every one of 119 existing input tables. The native unit test also exercises staging from DBC inputs with no Python files. All three asset caller tests passed. Removed `rust/tools/stage_control_dbcs.py` after these gates; candidate workflow uses the native command. Host radard tests/examples and all-targets clippy passed. ARM linked CLI/example/library tests passed; QEMU ran six library tests including source-preserving staging. Final bundle smoke remains deferred until integration freeze.

## Final ARS display metadata compatibility update

Replay now drains the native ARS interface warning queue after every update, including errors, through the same exported `native::Carlog` LOGPRINT filtering as production. Warnings remain console effects and are not conflated with synthetic input-error publications. Recorded and raw full-schema replay preserve `hasDisplayData` and the separate display point list in a display-only output field; this list is never supplied to the control tracker. Analysis inversion applies only the difference from the recorded flip, independently to both point namespaces. The shared decoder preserves identical point field promotion for each list.

Host and linked ARM tests pass the real Data-to-Cereal-to-replay display separation and exactly-once flip round trip, including equal track IDs in separate namespaces and an unaffected corner point. All seven radard library tests pass on host and ARM, including exact pinned DBC generation and native stage-dbcs; all-target clippy passes. The source fingerprint already includes all native source/schema/DBC bytes, so the new ARS provider and replay behavior invalidate old identities automatically. A full ARS source-to-raw-replay/display corpus and final integration-frozen package remain pending; these bounded round trips are not the whole source receiver or deployed NAS evidence.

## Final Bosch context and raw replay gate (2026-10-16)

The raw adapter now caches independently stamped livePose/modelV2 messages using
production `wire::{bosch_pose,bosch_model}` decoders. It supplies them to the
production Bosch interface before each matched CAN/ego update. Replay time
advances only on nonempty matched CAN batches, preserving future scene messages,
source expiry and context across interface rebuilds. Scene arrival never consumes
or replaces the nonconflated CAN/ego join. `replay/log.rs` supplies these optional
services while retaining its existing recorded-state processing.

`radar_raw_bosch_trace` exercises this actual adapter against the vehicle owner's
pinned complete-source real-IPC captures. HOST and actual ARM/QEMU each pass four
mode1/mode2 × flip0/flip1 families:880 matched batches and172 publications, every
control/display field, validity and Float32 bit pattern exact. Receipts are
`bosch-raw-{host,arm}1/receipt.json` under the planner scratch directory; they bind
the source capture and input hashes. The source process publication wall clocks
are intentionally reported separately from the offline recorded-timeline header;
these are payload-equivalence results, not whole-event byte equivalence. No
constructor clock adjustment or numerical tolerance was needed. Native example
builds are recorded in `bosch-raw-{host,arm}-build1.log`.

This closes the added Bosch scene input path for the captured replay corpus.
The vehicle owner's per-definition source coverage map and the remaining legacy
upload/export adapter review still govern whole-file coverage. The final local
package smoke remains deferred to the coordinated source freeze; no NAS or device
publication was performed.
