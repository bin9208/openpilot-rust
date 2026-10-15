# dev-md integration evidence (#338)

This is an incomplete upstream port, not an installable/accepted candidate.
The plan and source inventory are `dev-md-upstream-plan.md` and
`rust/upstream/carrot-wip.json`. No dev/main merge or device action was taken.

## Branch isolation and source accounting

Local `dev-md` was created from fetched `origin/dev` at
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` on 2026-10-15 KST.
The original `codex/fix-294-encoder-rotation` branch was preserved. Source was
fetched by URL into the read-only `carrot-source/carrot-wip` tracking ref;
origin remains `https://github.com/bin9208/openpilot-rust.git`.

The 371-commit/1,018-path inventory covers the exact source range, including
71 documentation-only commits. Non-runtime engineering docs remain in source
history; inherited upstream workflows are excluded to preserve isolated CI and
disabled publication. All functional omissions still need individual evidence.

CI routing adds dev-md to fast PR, integration, user-doc and Rust host/ARM
coverage. The existing required `fast checks` job fails before checkout when
the target branch is `dev`, preventing a PR carrying this track's workflow
from passing that required check. This is a branch-level guard, not a server
prohibition against manually copying arbitrary individual patches elsewhere.

Local RED: old routing failed three assertions and the new branch guard was
absent. GREEN: `python3 -m unittest discover -s tools/ci/tests -v` passed 32
tests, including actually executing the guard for dev, dev-md, main and an
issue branch. Existing source-only publishing guards remain tested and intact.
Exact-SHA cloud validation is still a separate gate.

The starting dev SHA's completed cloud runs were inspected: Fast checks
[38646726235](https://github.com/bin9208/openpilot-rust/actions/runs/38646726235),
Integration [38646726375](https://github.com/bin9208/openpilot-rust/actions/runs/38646726375),
user docs [38646726245](https://github.com/bin9208/openpilot-rust/actions/runs/38646726245)
and Rust port [38646726305](https://github.com/bin9208/openpilot-rust/actions/runs/38646726305)
all succeeded. These baseline results do not validate the new dev-md commits.

An independent GPT-6.1 Sol reviewer inspected the branch-isolation changes,
schema/sound boundary, VW ports/reference snapshots, NNFF retry and Wiki test
repair against source and reported no actionable findings. The reviewer ran
30 Python workflow/isolation/NNFF-reference tests successfully. PSS collection
was still in progress and explicitly excluded from that review.

A second independent review covered the completed PSS sampler, runtime cadence
and pinned reference fixture, with no concrete correctness findings. It verified
the two fixture files against the original source bytes and hashes.

Parent integration verification after all native edits: a single locked Cargo
run across cereal, beepd, soundd, selfdrived, runtime-core, proclogd and controlsd
passed 86 tests plus doc-test targets. Separate final steering math and VW
parameter/asset checks passed 6 and 5 tests respectively. Full control-policy
and card suites still require the numerical/Hyundai fixture prerequisites noted
in the steering record. Exact original/native procLog packet comparisons were
rerun successfully for both smaps_rollup and chunked smaps. Original steering
tests passed 19, and the retained NNFF reference tests passed 2. No runtime
CPU-performance claim follows from these results.

## Settings and Wiki

Paired settings summary and cruise-gap detail now describe VW MEB saved steering
ratio and single-point longitudinal gain behavior. The native `check-user-docs`
validator passed on the six changed Rust/guide paths using the existing host
binary; source-reference changes are validated separately before commit.

Read-only Wiki inspection at upstream Wiki commit
`a693b162aba2b3013f6fc6be7c7e29622d69ac98` confirms Korean/English CustomSR,
SteerRatioRate and LongTuningKpV/KiV/Kf manual explanations already describe this
same behavior. No live Wiki write or publishing workflow was needed or enabled.
The four CustomSR/SteerRatioRate pages passed the Markdown validator.

Generator validation exposed pre-existing stale hardcoded counts (184 versus
185 catalog parameters), tracked in [#339](https://github.com/bin9208/openpilot-rust/issues/339).
The tests now derive the expected parameter set directly from catalog JSON and
check complete generated coverage. Retired-setting rejection, read-only Wiki
behavior, locale coverage and Markdown validation remain intact. Before repair:
2 failures; after repair: all 25 generator/checker tests passed. The production
generator and catalog were not changed by this repair.

## Runtime work and limits

- Schema foundation: see `dev-md-schema-validation.md`; wire tests do not prove
  newly added fields have active producers/consumers.
- VW steering/tuning: see `dev-md-steering-validation.md`; missing full-suite
  fixture/numerical prerequisites remain explicit.
- NNFF notification and paced PSS collection have their own validation records.
- The control/model/UI audit tables identify still-unported subsystems. In
  particular DM2, radar decoders/selection, signal assistance, latest model/eGPU
  lifecycle, Jetlink v3, camera ownership, S3 and complete UI integration remain.

Keep #338 open until all functional coverage, integration and user acceptance
criteria are satisfied. Do not promote this branch to dev or main from this
engineering evidence.
