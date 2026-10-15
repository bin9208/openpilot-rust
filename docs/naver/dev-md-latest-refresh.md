# dev-md changed-runtime candidate, 2026-10-16

Issue: https://github.com/bin9208/openpilot-rust/issues/338

Source: `ajouatom/openpilot:carrot-wip` at
`8e7eec049241b6ba384e9197aacdfb01f7d88ed3`. This adds nine commits after
`e8e152dbcfd9574a1791ed12026d1880e87689b3`, covering 49 changed paths.
The cumulative source inventory contains 380 commits. Inventory counts do not
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

The ledger records a disposition for every one of the 49 latest paths, including
source tests/docs, upstream publication workflows and off-device installer/NAS
tools that are outside the runtime port. No upstream publication job is enabled.

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
