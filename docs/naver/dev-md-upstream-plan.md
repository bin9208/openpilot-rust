# dev-md latest Carrot Rust port (#338)

> Execution: continue the approved runtime design autonomously; implement and
> validate dependency-ordered groups, keeping all work isolated from dev.

Issue: https://github.com/bin9208/openpilot-rust/issues/338
Spec: [full runtime design](../rust-port/design.md), extended by the user's
2026-10-15 branch and source-selection instructions below.

## Contract and starting state

`dev-md` starts from `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`.
Do not advance `dev`, open a PR into it, or merge these changes into it.
User confirmation of correct operation is required before any main promotion.
No device access or deployment is part of this request.

Source is `ajouatom/openpilot:carrot-wip` at
`e8e152dbcfd9574a1791ed12026d1880e87689b3`, fetched 2026-10-15 KST.
The bin9208 mirror is stale at `43203371004e035bdb70a00a8dad29a4b657c6c3`,
which is also the common ancestor: 371 reachable source commits and 1,018 net
changed paths. Counts describe source history, not remaining Rust modules.

Use final upstream behavior, accounting for intervening reversions. Preserve
the native startup, updater, packaging and prior fixes. Python/Cython-specific
optimizations may be omitted only where inspected native ownership/algorithms
already achieve equivalent behavior; Rust alone is not evidence. Do not import
source workflows, publication destinations or experimental branch behavior.
Keep source provenance, schema ordinals, safety behavior and external providers
explicit. No CPU savings or device acceptance follows from host tests.

## Dependency-ordered work

- [x] Create `dev-md` from the fetched stable dev tip; preserve original branches.
- [x] Pin source and common ancestor; create issue #338.
- [x] Inventory every commit and net changed path, with disposition/evidence.
- [x] Add CI coverage for dev-md and a required-fast-check rejection of this
  track when its target is dev. Keep publishing guards intact.
- [ ] Port additive wire-schema, Params and shared support contracts first;
  preserve existing fork ordinals and regenerate native consumers through the
  existing schema build. Test old/default and new field decoding.
- [ ] Port vehicle/radar/control/planner final behavior with deterministic
  reference fixtures and native regressions. Track replay-service compatibility
  without deploying experimental branch changes to the stable NAS service.
- [ ] Port model/eGPU/Jetlink failure recovery, selected model contracts and
  signal observation/assistance with numerical and fault-lifecycle tests.
- [ ] Port monitoring/selfdrived/manager lifecycle behavior, including restart,
  shutdown and missing-input cases.
- [ ] Port UI/web/cluster/S3/settings consumers after their producer contracts.
  For changed settings update paired Korean/English guides and Wiki explanations.
- [ ] Validate affected host crates, exact-head Actions and complete ARM runtime
  package/startup/recording/upload composition; record all failures and limits.
- [ ] Independently review the full branch; resolve material findings. Keep
  issue open for remaining native implementation or user acceptance.

## Review focus

1. Missing producer/consumer fields must not silently disable a new feature.
2. Reset/reconnect/offroad transitions must clear stale signal/radar/model state.
3. Native performance changes must preserve arithmetic, ordering and safety
   branch decisions, including non-finite values and delayed/stale inputs.
4. New default values must match source while preserving existing saved Params.
5. Branch and package provenance must prevent this candidate reaching dev or
   a vehicle through normal stable update/publication jobs.

## Working record

Initial native slices are implemented and host-tested: additive schemas with
an explicit fork event-number adaptation; VW manual/scaled steering ratio and
single-point Ki; deferred one-shot NNFF startup notice; paced incremental PSS
collection. They do not complete the subsystem tasks above. The source coverage
ledger retains all outstanding work; no full-runtime acceptance is claimed.

The three subsystem audit documents refine the concrete file/test task lists.
`rust/upstream/carrot-wip.json` is the coverage ledger. `pending` is an explicit
unimplemented/unverified state; changing the source pin does not confer parity.

Disk at setup: over 366 GiB available. Check again before every build/install;
retain 25 GiB plus growth, recover at least 35 GiB if below the threshold.
Use `/home/fma1/rust/cache/cargo-target`, bounded jobs, no incremental builds.
Do not run simultaneous Cargo operations against this coordinated cache.

Docs-Not-Needed: engineering scope and branch-isolation record; no user setting
behavior is changed by this plan.
