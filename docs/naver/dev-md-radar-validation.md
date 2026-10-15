# #338 final-source radar port

Pinned source ajouatom/openpilot e8e152dbcfd9574a1791ed12026d1880e87689b3; delta baseline 43203371004e035bdb70a00a8dad29a4b657c6c3. dev-md isolation applies. No stable NAS rollout or device access is authorized for this candidate. Experimental replay requires local package verification.

| Source path under openpilot/selfdrive/carrot/radar_motion | Native coverage | Evidence/status |
| --- | --- | --- |
| stationary_lane.py | radard/src/stationary_lane.rs, shared confident_ego_lane and stopped-corner tracker | Host native full-state differential passed (17,758 tracker steps and 7,098 controller actions); ARM full-state comparison passed; package pending |
| in_lane.py | radard/src/in_lane.rs | Host native full-state differential passed (17,758 tracker steps and 7,098 controller actions); ARM full-state comparison passed; package pending |
| controller.py | radard/controller/{mod,update,forced_scc}.rs | Stopped/in-lane L2 hooks, SCC-only selection and forced-SCC lateral/refusal continuity implemented, including closing-range bypass; 7,098 full-state controller actions passed; ARM full-state comparison passed; package pending |
| predictor.py | radard/path/geometry.rs | Spatial terminal tangent implemented; full-state/cache controller comparison passed; Cython/Python 3,340 cases passed; ARM full-state comparison passed |
| primary.py | existing radard/primary modules | Final closing-range continuity, corner motion history, paired vision tolerance, SCC longitudinal-only semantics and held front moving lead changes implemented; 7,098 full-state controller actions passed; ARM full-state comparison passed; package pending |
| trajectory_cutin.py | existing radard/trajectory_cutin modules | Final mathematical Python fallback delta is wrapper-only; Cython/Python equivalence passed 3,340 cases; Rust full-state/controller comparison passed; ARM full-state comparison passed |
| native.py, _motion_native.pyx, SConscript | Rust-owned path and cut-in kernels | Original accelerated/fallback kernels matched bit-exactly in 3,340 cases, with Rust full controller/state equivalence in 7,098 actions; ARM full-state comparison passed |

Model wire decode now retains lane lines, standard deviations and lane-change state needed by exact confidence gating. Source-level coverage is not completion: host and ARM regression/differential now pass; daemon-specific source integration and local replay package checks remain. Shared confidence gate also feeds selfdrived warning work owned by the monitoring agent.


Reference preparation passed 843 final upstream tests across seven suites. Exact source fixture records 7,098 full controller actions/342 owners and 17,758 direct tracker actions (including explicit resets), with state/cache snapshots. The source-only preparation is complemented by the native host and ARM comparisons below. The daemon-specific `test_radard_dpath.py` suite remains pending because the retained Python opendbc enum lacks final `HyundaiExtFlags.BOSCH_RADAR`; it was not silently counted as covered.

The exact pinned `_motion_native.pyx` was compiled in scratch with Cython 3.3.0, system C++ and `-ffp-contract=off`. `check_radar_native_equivalence.py` passed 3,340 bit-exact accelerated-Cython versus final-Python-fallback comparisons (median slopes, complete motion metrics, nearest-segment projection). This establishes the original optimization's mathematical equivalence over those cases; Rust-to-final full-state comparison is recorded below on host and ARM; this establishes corpus equivalence without claiming measured target performance. Disk checked at 349 GiB before this small extension build; no dependencies installed.

## Host native verification

Actual `radard_controller_trace` comparison passed **7,098 controller actions/342 owners**, comparing entire output, all tracker/matcher/controller state, process-wide path-cache fingerprints and exact float bits against pinned final source. Actual `radard_lane_trace` additionally passed **17,758 direct tracker actions**, including reset behavior, with complete states and outputs. This directly covers the new source tracker suites rather than counting their Python assertions as native validation. Both daemon and trace examples built. Host radar tests passed all 12 (including new current-lane/current-ambiguity/repeated-measurement regressions), and combined planner/radar all-target Clippy with warnings denied passed.

Together with 3,340 bit-exact original Cython/Python kernel comparisons, the full native state comparison establishes equivalent Rust-owned kernels for the original optional native wrappers over this corpus; no original-runtime Cython module is integrated. Primary/controller, terminal-path geometry, stationary_lane/in_lane and optimized cut-in math now have host equivalence evidence at source commit e8e152dbcfd9574a1791ed12026d1880e87689b3. Final daemon-specific Bosch reference coverage and source-matched local replay-package verification remain explicit gates.

Local replay-package inspection found the old `tools/carrot_route_vault/build_bundle.py` exports committed Python/Cereal/DBC files and excludes Rust radard/decoder binaries. A bundle of current HEAD would therefore not validate this uncommitted native candidate. Native replay packaging/adapter is an explicit remaining implementation/verification gate; do not substitute the stable Python bundle or deploy it to the NAS.

## ARM full-state source comparison (2026-10-15)

The final-source radar controller fixture also passes under linked ARM/QEMU: **7,098 actions across 342 owners, zero differences**, preserving complete outputs, controller/matcher/tracker state, cache bookkeeping and floating-point bit patterns. The separate stopped/in-lane tracker fixture passes **17,758 actions including 8,346 resets**, also with zero differences. Evidence is under the current planner scratch directory in `radar-controller-arm/{actual,receipt}.json` and `radar-trackers-arm{-receipt.json,.json}`. These are final pinned source fixtures, not a replay against the retained older Python runtime.

The standalone native local replay production interface separately matches all output fields and float bits for **6,696 frames/327 compatible source owners** on host. Cases requiring source-test predictor/reset injection remain in the full controller reference fixture. Native full-Cereal replay integration tests pass on both host and ARM QEMU. DBC native generation and complete raw-CAN/provider packaging are the next local gates; Bosch/ARS source decoder/daemon additions remain coordinated with the vehicle owner. Stable NAS deployment and device validation are not authorized for this track.

## Owner coverage ledger for central inventory

These entries close only the named radar-motion file groups at final source `e8e152dbcfd9574a1791ed12026d1880e87689b3`, with host **and ARM** complete-state reference evidence (7,098 controller actions; 17,758 tracker actions) and the separately recorded 3,340 native/fallback kernel cases. They do not close every file in a mixed upstream commit.

| Upstream change commit | Verified radar-motion group | Remaining outside this group |
| --- | --- | --- |
| `5a33dc40ed1cdf5ba0ca5aacbe7c0200537a9763` | `controller.py`, `in_lane.py`, exact source suite | replay packaging/cluster validation-case migration; inherited publisher intentionally isolated |
| `734634c98383455228a8452dd8c943117f484760` | `stationary_lane.py`, ambiguity/reset behavior | final package integration |
| `99141237a2494606e63425edb55f7367ba26ed5b` | `controller.py`, `stationary_lane.py` | monitoring warning owner; complete source replay/export adapter policies and package |
| `910bd82fb744999020c769f6e7e6379826c490af` | closing-range `controller.py`/`primary.py` final behavior | planner numerical/integration limits recorded separately; no superseded braking-onset memory restored |
| `e6a6284437a76c2d60bda75efa4cb2cf6e85ef07` | `primary.py` corner motion corroboration | final package |
| `b3354534acebe83179730d1f3675dd047a2fe83e` | SCC-longitudinal `controller.py`/`primary.py` | daemon-specific source suite, complete replay/export adapter, setting/docs owner |
| `50eac829e8420b12b4231228ab7f03557aaeb083` | held moving front lead in `primary.py` | cluster case/export/package integration |
| `5f9a8b3622a304908c1dca051d2b384cddeb833b` | tentative-front rejection in `primary.py` | cluster case/export/package integration |
| `1b7dbbd3fef8e18f24e203971e1df402ead3f07e` | terminal path tangent in `predictor.py` | cluster case/export/package integration |
| `bc0aa059ccf2a300114da77a4bf9801b475231ca` | Rust replacement for `native.py`, `_motion_native.pyx`, predictor/cut-in kernels | CAN owner; daemon/export/build-tool groups; no global optimization or performance claim |

`radar/radard_dpath.py` is deliberately not marked closed: final Bosch/ARS configuration and the complete source daemon suite remain. Native DBC generation is independently verified against actual final generated bytes (103 tables/45 compositions); its build/test caller migration does not prove radar decoding for a new vehicle protocol.

### Final daemon subscription delta identified

The final source daemon adds a distinct subscription policy for Hyundai Bosch (`extFlags & 1<<17`) with positive effective RadarTracks: poll both `modelV2` and `liveTracks` to drain independent conflated 20 Hz streams, while controller work/publication remains strictly model-triggered. Native `wire::subscriptions` and the runtime hook are now implemented, with a 192-case exact pinned-source function comparison and native subscription/model-only publication regressions prepared. Their next host/ARM compile and execution are pending; this entry is not a passing gate yet. Exact final Hyundai enum and radar mode helper bytes are added to the source fixture for the comparison. No input-frequency or validity threshold is relaxed.

2026-10-15 daemon polling closure: exact final-source `bosch_radar_tracks_active` and `effective_radar_track_mode` match native subscription selection for 192 CP/mode cases on both host and ARM QEMU. Native tests preserve livePose alive/valid ignores, retain frequency checks, and publish only for model updates. Real isolated msgq tests drain twenty Bosch track-only wakes without publication and then publish once for the model wake, on host and ARM QEMU (three polling tests each). ARM's first IPC attempt lacked the explicit test namespace and failed before creating the publisher; creating the isolated `/dev/shm/msgq_rust-probe-radard-arm-poll` namespace produced the passing rerun, with cleanup afterward. These gates cover the final daemon polling delta, not Bosch/ARS CAN decoder validation or the complete legacy replay policy adapter.
