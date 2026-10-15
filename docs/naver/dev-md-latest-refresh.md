# dev-md changed-runtime candidate, 2026-10-16

Issue: https://github.com/bin9208/openpilot-rust/issues/338

Source: `ajouatom/openpilot:carrot-wip` at
`8cf5570f6962bd9cb17ec384bbfa12a3a167a5a8`. This adds ten commits after
`e8e152dbcfd9574a1791ed12026d1880e87689b3`, covering 64 changed paths.
The cumulative source inventory contains 381 commits. Inventory counts do not
mean every baseline API, external service or development tool must be rewritten.

## Latest changed behavior

- MD v3 selects the verified `comma4-big-md-v3-1365126a` manifest and
  `d4c7a3fcfc95813826ca871a80bea18df833aacb2eb6f5333e25a01e65b1a422`
  checkpoint. Runtime admission accepts the selected runtime identity, preserves
  the old explicitly selected model, and retains the native USB wait policy.
  The web label and generated bundles identify MD v3.
- Tesla uses final emitted steering feedback, the strict Float32 25 km/h
  cooperative boundary and corrected safety permission logic. Angle saturation
  receives the actual controls steering-limit flag.
- S3 startup/shutdown confirms the required sync/brightness acknowledgements;
  autorun dims a registered S3 before USB scanning and reports expected transport
  retries concisely. Authentication/protocol failures retain ordinary reporting.
  Selected C firmware and project version match upstream 1.1.0.
- Python frozen-traceback GC repair needs no native GC simulation: the Rust JPEG
  pipeline owns/joins its worker, shared data has no pipeline back-reference and
  errors do not retain Python frames. This is ownership evidence, not measured
  memory or CPU savings.

The ledger records a disposition for every one of the 64 latest paths, including
source tests/docs, upstream publication workflows and off-device installer/NAS
tools that are outside the runtime port. No upstream publication job is enabled.

## Final signal delta (`8cf5570f`)

The final remote check found one additional source commit after `8e7eec04`.
Its observation HUD now reads the display transport independently of planner
intervention and longitudinal-control activation. Current colors, dim previous
colors and unknown gray follow a two-second capture-based lifetime; repeated
packets and later unknown packets cannot extend a previous color. Session or
sequence rollback clears that memory. It remains display-only. The main UI,
overlay invalidation and USB cluster ring/caption share this summary.

Preparation entry and the final acceleration cap both exclude either active
lead at or below 50m or with a non-finite range. Far leads remain MPC obstacles.
Soft hold prevents entry, and the final cap independently requires no soft hold
and speed strictly above 5km/h and below 82km/h. These conditions do not issue a
full stop or a departure command. Classical observation survives mode 1/2 changes;
mode 0 still stops it. The unchanged neural owner and automatic backend policy
retain their source behavior.

The setting catalog and paired Korean/English guide describe these exact trial
semantics. Existing Wiki generation consumes the updated localized catalog;
no stable Wiki is published from this candidate. Source-reference snapshots and
the CI fetch list include the new pinned commit, without converting any tool.

Focused planner checks pass on HOST and actual ARM/QEMU: eleven signal tests,
48 longitudinal cases / 5,760 frames with 44,385 exact floating comparisons,
and 360 full Carrot/MPC owner frames including both near/far leads and soft hold.
Strict planner library Clippy passes. Receipts are under
`.analysis/scratch/2026-10-16-dev-md-delivery/planner-8cf-*`.

HOST and actual ARM/QEMU display comparisons also pass: seventeen summary
sequences (capture expiry, repeats, unknowns, session/sequence reset and off),
sixteen visibility combinations, 288 existing signal combinations and 384
unchanged-input overlay snapshots with the added summary tuple. Cluster ring
colors are compared with the actual new source renderer method. The existing
mode-0 indicator regression still matches all 160 frames, using its prior
render executable only for that unchanged mode. Newly built display probes
cover changed behavior; affected UI examples and the cluster library link for
ARM and strict Clippy passes. Receipt: `signal-8cf-receipt.json` under the same
delivery scratch directory. The final package rebuild includes actual UI and
cluster executables; these checks do not claim vehicle execution.

The existing classical-owner lifecycle probe passes on HOST and actual
ARM/QEMU: the same worker survives `1→2→1→2`, then exits on mode 0. Its existing
comparison-selection exit, exclusive lock, stale/duplicate input rejection and
backend kill/join checks remain active. Observer all-target strict Clippy passes
on HOST and AGNOS GNU. Logs: `signal-8cf-{host,arm}{,-build,-clippy}.log` under
`.analysis/scratch/2026-10-16-dev-md-package/`. The runtime change removes only
the classical mode snapshot and equality check; the neural owner is unchanged.

## Scope correction and checks

The user's correction excludes repository-wide tool retirement, untouched API
parity and exhaustive project verification. Unrelated Jetlink host-server/tool
rewrites, Panda test-tool conversion, signal cache converters, S3 provisioning
and firmware-builder rewrites, standalone Shell85 and diagnostic examples were
preserved privately under
`.analysis/scratch/2026-10-16-dev-md-scope-excluded/` and excluded from this
candidate. Existing DBC preparation scripts and their callers were restored.
Required runtime/package compatibility updates remain. The separate #296
worktree was not edited or messaged.

Focused results from this continuation:

- Tesla three native regressions pass on host and ARM/QEMU; focused upstream
  suite 58 tests/78 subtests and ARM safety 36 checks pass.
- S3 20 source lifecycle cases, retry-wrapper regression and three display
  continuity regressions pass on host and ARM/QEMU; affected ARM daemons link.
- Model selection's existing 750 cases pass on host and ARM/QEMU after updating
  only the changed source manifest constants. New MD v3 artifact preparation and
  worker admission evidence is in the model runtime record below.
- ARM checks for controlsd and the affected model/cluster/signal paths passed.
  After removing tool-only registrations, an additional ARM library check for
  Jetlink, cluster-wifi, dev-tools and radarcan passed.
- Existing web model tests and direct MD v3 label check passed. Existing user-doc
  validator passed for the changed tracked paths. Diff whitespace check passed.
  The stored Wiki patch's blank context lines are intentional unified-diff
  syntax; it is checked separately with `git apply --numstat`.

Detailed evidence: [vehicle](dev-md-vehicle-validation.md),
[model/runtime](dev-md-model-runtime-validation.md),
[cluster/S3](dev-md-cluster-transport-validation.md),
[signal](dev-md-signal-validation.md), [camera](dev-md-camera-validation.md).
Earlier records contain historical broader checks; these were not restarted as
a new delivery requirement.

## Explicit limits

- Optional signal QCOM inference requires authenticated converted native graph
  bundles; original PKL/JSON caches are not directly accepted. No authentic
  positive QCOM GPU inference was performed. Classical/default and CPU paths
  are unaffected by this optional input-format limitation.
- Off-device native radar replay currently needs explicit mode selection;
  automatic mode discovery from recorded initData remains outside this runtime
  candidate. No stable NAS service was changed or published.
- MD v3 prepared assets are model-specific; an old model package cannot be
  relabeled as this candidate. A prepared model asset directory is not a complete
  installed vehicle runtime release.
- The historical whole-file ledger retains `coverage_complete: false`; it must
  not be presented as whole-project parity or successful device testing.
- `dev` stays at `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. Changes belong only
  to `dev-md`. No main merge, device connection, deployment or driving test was
  performed. User testing remains the acceptance step.
