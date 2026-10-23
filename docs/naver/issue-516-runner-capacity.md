# Genuine runner capacity rejection (#516)

[Issue #516](https://github.com/bin9208/openpilot-rust/issues/516) follows
[required ARM job120220822740](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/job/120220822740)
at PR500 source `10919538d31b04d28d9a23b383d925441434d579`.
Before compilation, free space was 9,682,640,896 bytes and estimated operation
growth was 12,884,901,888 bytes. The growth admission correctly rejected it.
The retired 25 GiB reserve is unrelated; the 12 GiB estimate remains unchanged.

The isolated `codex/fix-516-runner-capacity` branch starts from that candidate,
reusing the completed #503 proof worktree while retaining its original branch.
No PR500 source change or other-session CI cancellation is required for storage
recovery. The first workflow is read-only inventory, not a recovery or build.
Only this unmerged operational branch is excluded from Fast's push trigger, so
its normal clean checkout cannot erase evidence before inventory or replay
unchanged successful tests. Fast's PR trigger and PR500 source stay unchanged.

`runner-capacity-516.yml` uses only registered bin-actions. Checkout preserves
ignored build/cache evidence. Standard owned job-temp initialization, upload and
cleanup retain the inventory JSON. The isolated system Python reads filesystem
metadata and bounded `du` output for fixed cache/toolchain paths and project
job-temp directories. A privileged GNU timeout owns each reader's process group,
with at most 20 seconds followed by a two-second kill escalation; the outer
process wait allows five seconds of grace. The total metadata budget is
180 seconds; omitted roots or incomplete measurements are explicit.
Only complete NUL-delimited metadata records are parsed; interrupted tails are
discarded with an explicit flag so a timeout cannot prevent the receipt.
It records overlapping measurements separately, not as a sum, with
UID/device/inode and readable job-owner markers. It reads no file
contents except validated job-owner IDs. No cache, source, toolchain or SDK is
deleted; only its own inventory receipt is created.

Actual Actions inventory must identify any unused/reproducible recovery targets.
Before deletion, establish their ownership, completed job state and absence of
current users, and verify that no active task needs them. In particular, the
interrupted #296/#514 source/guest/target/cache remains in use for continuation
even after its originating run fails, is cancelled or completes. Preserve that
state, source/locks, installed tools and needed current evidence. Keep the
original capacity/overflow/source/ABI/seal checks.
After reviewed recovery, selectively retry the failed exact-source ARM job and
dependent gate when the original workflow permits it. Successful unchanged jobs
are retained. Normal PR500/dev integration and post-merge acceptance still apply.

Preparation checks are YAML/Bash/Python syntax and diff review only. No local
functional test, fixture, build, dependency installation or cache deletion ran.
The first registered inventory completed at source
`d2d2918c29284e02a7b0047c3c75a74db818b589`,
[run39907008163](https://github.com/bin9208/openpilot-rust/actions/runs/39907008163),
with artifact12481269976 SHA-256
`6f6ba8b620a676120aee8c0188af126d56d986528ace8f7076c17bdcb5776ae3`.
It measured 4,582,891,520 free bytes, 5,643,464,704 allocated bytes under the
shared checkout's Git object directory and 2,685,640,704 bytes of user caches.
These are overlapping metadata, not a recovery result. All large project
job-temp owners inspected were other #296 work and remain preserved.

The next revision replaces the filesystem survey with a focused read-only Git
metadata probe. It records pack identities/sizes/link counts, `count-objects`,
ref digests before/after and the unique object-ID digest and size totals,
including unreachable objects. It does not assume the packs are redundant.
The object metadata file has a 64 MiB file-size ceiling and the Git reader has
a 120-second timeout plus a two-second escalation; only its compact receipt
is uploaded. No object contents, repack, prune, ref edits or cache deletion
are performed. Installed tools and interrupted project state remain intact.
See the primary [Git object metadata documentation](https://git-scm.com/docs/git-cat-file)
and [Git storage counts](https://git-scm.com/docs/git-count-objects).
Actual Git probe, reviewed recovery and selective ARM retries remain pending.

The same diagnostic job also records the independent
[issue #517](https://github.com/bin9208/openpilot-rust/issues/517) startup bridge
failure at the sudo boundary. It uses the installed action Python to capture
normal, unchanged legacy-sudo and explicit-loader-path candidate-sudo version,
prefix, executable, UID and four installed module identities/import errors.
The candidate must match normal Python and run as root; the legacy result is
recorded as observed without requiring a fabricated failure. This invokes no
project code, installs no dependencies and changes no runtime behavior.
Actual bridge/Athena acceptance still requires their complete existing jobs.
