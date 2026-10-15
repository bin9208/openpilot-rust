# Initial lateral readiness and published DM disable consumer — #338

Source target `e8e152dbcfd9574a1791ed12026d1880e87689b3`, compared with ancestor `43203371004e035bdb70a00a8dad29a4b657c6c3`. Relevant source commits: `e7987ed0` (startup protection), `7ceee116` (initial-only one-way latch), `c60cde06` (Toyota activation/event cadence), plus final controlsd published `dm2Disabled` forceDecel consumer. Work is confined to dev-md; no dev/main changes, device action, commit or push by this slice.

## Native behavior

`control-policy::lateral_readiness` holds the typed shared policy. A service must have been received and pass existing subscriber checks; onroadEvents instead requires raw receipt, validity and liveness because legitimate event-change bursts exceed its 1 Hz frequency range. Initial steering also requires valid/sensor-valid/posenet-valid live parameters, finite ratio/stiffness/offset/roll with ratio >1 and stiffness >0, CAN validity/no steering faults, four finite car steering/speed values, no selfdriveInitializing event, all three pose flags, finite model curvature, and—when lane lines are requested—a fresh valid 17-element solution with every psi/curvature/distance finite.

Controlsd and card own separate process-lifetime latches. Later input failures or engagement changes do not rearm them; existing runtime policy still handles those failures. Controlsd uses nominal CP ratio/stiffness=1/offset=roll=0 for inactive calculations while live values are unusable before readiness, and initializes desired curvature from current curvature on its first active lateral update. The selected effective geometry is passed through the lateral-controller request as upstream passes its selected lp, including inactive PID telemetry.

Card adds liveParameters/livePose/selfdriveState/lateralPlan subscriptions and independently checks readiness at the final application boundary. A queued lateral command before readiness loses latActive, torque and curvature, and requests current steering angle; enabled/longActive/longitudinal acceleration remain unchanged. The shared lane view borrows controlsd's decoded arrays; card avoids array decoding once latched.

ControlsState.forceDecel now uses `(!driverMonitoringState.dm2Disabled && alertLevel==three) || softDisabling`. It no longer reads raw DisableDM or suppresses soft-disabling deceleration when DM is disabled. No driver-monitoring producer policy is implemented in this slice.

## Test-first evidence and current verification

RED:

- New shared-gate test initially failed E0432 for absent readiness module.
- Native card core test queued latActive/torque=0.7/angle=99/curvature=0.01 without readiness; baseline applied torque 0.7 instead of required zero.
- Added DM packet regression initially failed for missing published-disable input/helper.
- Complete controlsd source differential exposed an integration error after the first implementation: VM used nominal fallback while lateral PID telemetry still used original live roll/offset. Source row0 angleError was 0; native was 0.344995. Root cause was the missing effective-lp argument to lateral::Request, not an oracle tolerance issue. The argument is now threaded explicitly; the focused 51-frame reproduction and final full source comparison both pass.

GREEN obtained so far:

- Full control-policy suite: 12 tests (4 readiness, 6 math, 2 real pinned native numerical artifact tests), doc-tests passed.
- Full controlsd suite: 8 tests, including all eight disabled/distraction/soft-disabling packet combinations; doc-tests passed.
- Native card core: 5 tests plus independent final-boundary helper test passed, including preserving enabled/longActive/acceleration and never rearming after later stale inputs.
- 71 exact-source differential gate cases passed: receipt/check policy, onroadEvents exception, live flags/limits, finite car/model/lane values, individual array lengths, optional lane input, one-way latch and nominal inactive geometry.
- Card complete source differential: 7 cases / 5,928 steps passed, including a queued steering startup/recovery/later-invalid-service trace. Native core trace SHA256 `c4d25a96a503b1dbe0ca71d1d13105a05aeac62a57a264aea35b73ba6719065a`.
- Follow-up native tests: card native library 22, card core 5, controlsd 8 tests passed (35 total). Three trace examples rebuilt; all-target clippy across control-policy/controlsd/card with native features and warnings denied passed on final changes.
- Final full controlsd differential passed 44 cases / 8,790 frames / 716,822 numeric comparisons, maximum absolute difference `1.0880185641326534e-14`; this includes first-active initialization, nominal inactive geometry, later invalidity, published DM disable and soft-disabling. Focused reproduction passed 2 cases / 51 frames at max difference `4.440892098500626e-16`.
- Final malformed-startup source/native comparison passed 26 rejection cases. Malformed raw DisableDM is now an explicit accepted case because final controlsd no longer consumes it. The zero-torque-factor rejection runs past readiness acquisition rather than testing a permanently inactive loop. Synthetic zero-denominator source cases emit expected RuntimeWarnings; failure/sanitation behavior remains compared.
- Final card full source differential passed 7 cases / 5,928 steps again after optional-plan freshness short-circuit changes. Rebuilt core trace SHA256 `93f9835a775ffa7eed65e773c8a6972ecf349f013d4c2196e33cd211fdef76d0`.
- Full linked AArch64 GNU/Linux `openpilot-controlsd` and `openpilot-card` daemon binaries built successfully with native features, existing SDK wrappers and two jobs. ELF inspection confirms AArch64 PIE executables with `/lib/ld-linux-aarch64.so.1`; this is cross-build evidence, not AGNOS ABI or device execution.

Host numerical artifact was staged from the exact uv.lock NumPy 2.5.3 cp312 manylinux x86_64 wheel, SHA256 `b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3`, using existing native stage tooling. No dependency installation. Existing generated DBC test fixtures were staged only when absent; existing files were preserved. All builds used the coordinated target, two jobs, incremental disabled and disk guards (~365 GiB free). The repeated-load numerical test completed normally in 73 s; it was not skipped or killed.

## Source oracle integrity

`rust/upstream/lateral-readiness-source.json` retains exact final source files with repository/SHA, Git blob IDs, SHA256 and unchanged content as an explicitly approved test-only fixture. `lateral_readiness_source.py` verifies these before execution. Existing controlsd/card source oracles retain baseline methods and splice only exact final readiness, nominal fallback, first-active initialization and approved published-DM consumer AST fragments; unrelated pending DM/impact changes are not silently introduced into the oracle.

Controlsd's prior fixtures omitted live/pose/MPC validity and sent nominal 20 Hz services at 100 Hz. Under the real readiness policy that would compare two permanently inactive loops. Controlsd-specific fixtures now supply valid inputs at their actual 20 Hz rate, retain 100 Hz onroadEvents bursts, expose native/source latch flags and assert startup recovery and persistent readiness. Added source scenarios cover invalid-live nominal fallback, recovery, later invalidity, exact lane-plan length and published DM-disable transitions. The shared paramsd fixtures remain unchanged.

Complete runtime integration, AArch64 execution/AGNOS ABI and device acceptance remain parent-task gates. This slice makes no driving, device or performance acceptance claim.
