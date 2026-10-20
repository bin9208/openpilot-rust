# dev-md LFS checkout integrity — issue #415

Issue: [#415](https://github.com/bin9208/openpilot-rust/issues/415).
Inspection: 2026-10-21 00:30:37–00:31:45 KST
(2026-10-20 15:30:37–15:31:45 UTC).

## Result and limits

The tester reported HTTP 404 while resetting the existing original openpilot
checkout to `refs/remotes/rust/dev-md` (the independent repository is named
`rust`, while `origin` may still name an original repository) for
`v_asm_model.onnx`, SHA-256
`5d20cdbb457ba18db51a537ee2e305bbe442264b1613956068d473e35d15900d`,
6,166,229 bytes. At inspection time **all seven current dev-md LFS objects
were available from bin9208/openpilot-rust**, including that exact model.
Each downloaded into initially empty temporary LFS storage and matched both
the pointer's SHA-256 and byte count. Total: **104,864,948 bytes**.

- Source: `origin/dev-md` = `a2b06522a24f4a3533192daf99ac48f119782bda`;
  `git ls-remote` agreed before and after the inspection.
- Worktree: `/home/fma1/.codex/worktrees/dev-md-lfs-onnx/openpilot-rust`,
  shared branch `codex/fix-415-416-lfs-onnx`. No branch switch or reset occurred.
- Only remote: `origin`, fetch/push URL
  `https://github.com/bin9208/openpilot-rust.git`.
  LFS endpoint: `https://github.com/bin9208/openpilot-rust.git/info/lfs`.
- Missing during this audit: **0**. Uploaded by this task: **0**.
  No `git lfs push` was necessary or executed. The 404 was not reproduced
  **at the independent endpoint**. Follow-up read-only requests reproduced
  an object-level 404 at both original repositories; see the endpoint
  investigation below. There is no evidence here that the object was ever
  missing from the independent repository.
- This is asset download/integrity evidence. No functional tests, builds,
  Actions dispatch, source push, device connection, installation or runtime
  validation occurred. Tester checkout retry remains user-owned.

## Complete pointer inventory

`git lfs ls-files --long origin/dev-md` and a whole-tree search for the LFS
pointer version line identified the same seven paths. Local availability
below refers to `/home/fma1/rust/openpilot-rust/.git/lfs/objects` **before**
the isolated downloads. Every locally available object was also hashed and
size-checked. Remote results came from the independent repository only;
automatic remote detection/search was disabled during downloads.

| Tracked path | SHA-256 / LFS OID | Bytes | Primary local cache | Independent download | Uploaded |
| --- | --- | ---: | --- | --- | --- |
| `docs/assets/comma-logo.png` | `f54dec485ad298ee17b533354ed6f1fdfd3711a163d2c19d758f2ae340a0276e` | 57 | Absent | Verified | No |
| `openpilot/selfdrive/carrot/web/assets/alert_camera.svg` | `f1086279a32f49f68de1db4e89a93aa2aae779395cfb908e5f6a0cbf2bf7dae6` | 989 | Verified | Verified | No |
| `openpilot/selfdrive/carrot/web/assets/alert_police.svg` | `d0dea6745d5cd5cf8206ced256310fed587555b3309d80cad46a4c066a931401` | 707 | Verified | Verified | No |
| `openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx` | `d3761c185daf33897ff3ff5edf28115c5dc829c06c538033accb616d6c67528c` | 13,194,895 | Verified | Verified | No |
| `openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx` | `5d20cdbb457ba18db51a537ee2e305bbe442264b1613956068d473e35d15900d` | 6,166,229 | Verified | Verified | No |
| `openpilot/selfdrive/modeld/models/driving_supercombo.onnx` | `f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5` | 60,792,584 | Verified | Verified | No |
| `openpilot/system/hardware/tici/updater` | `273785b6fa93585f1a6f7bff25f253abdafafdcdbba25c4369db6c52a0f00daa` | 24,709,487 | Absent | Verified | No |

An additional unauthenticated POST to the independent repository's
`/info/lfs/objects/batch` endpoint, with `operation: download`,
`transfers: [basic]` and these seven `{oid, size}` pairs, returned a download
action and no error for every object. Signed download URLs and credentials
were not retained in the evidence. The batch response alone was not used
as proof of downloaded content.

## Inspection commands and storage

Read-only repository checks, run from the shared worktree:

```bash
git status --short
git branch --show-current
git remote -v
git rev-parse --git-common-dir
git ls-remote origin refs/heads/dev-md
git rev-parse origin/dev-md
git lfs version
git lfs env
git lfs ls-files --long --size origin/dev-md
git grep -l '^version https://git-lfs.github.com/spec/v1$' a2b06522a24f4a3533192daf99ac48f119782bda --
df -B1 . /home/fma1/rust/openpilot-rust
```

Available space was **118,008,430,592 bytes**, exceeding 25 GiB plus the
conservative growth estimate of **331,372,060 bytes** (three complete copies
plus 16 MiB). Space was checked again immediately before downloading.
No dependency installation or build was performed.

For each inventory entry, the exact pointer was read with
`git show a2b06522a24f4a3533192daf99ac48f119782bda:<tracked-path>`.
Its `size` and `oid sha256` fields were compared with the local cached
object's `stat().st_size` and Python `hashlib.file_digest(..., 'sha256')`.
The isolated storage path did not exist when the audit began. The following
command was executed separately for **each of the seven paths**, substituting
that literal tracked path for `<tracked-path>`:

```bash
git \
  -c lfs.storage=/home/fma1/.codex/worktrees/dev-md-lfs-onnx/openpilot-rust/.analysis/scratch/2026-10-21-lfs-onnx/before-lfs \
  -c lfs.fetchrecentalways=false \
  -c lfs.remote.autodetect=false \
  -c lfs.remote.searchall=false \
  lfs fetch --include='<tracked-path>' --exclude='' origin a2b06522a24f4a3533192daf99ac48f119782bda
```

Every command exited zero. Each resulting
`before-lfs/objects/<oid[0:2]>/<oid[2:4]>/<oid>` was independently hashed and
size-checked against the source pointer. `git lfs fetch` did not materialize
or alter worktree assets. Temporary binaries are reproducible and removed
after verification; concise JSON results are retained privately with an index
under `.analysis/archive/2026-10-21-lfs-onnx/`.

If a future audit actually identifies missing objects, first verify the
local source object's hash/size and the independent origin/LFS endpoint,
then upload only those exact OIDs. The authorized repair form is:

```bash
git lfs push --object-id origin <verified-missing-oid> [<verified-missing-oid> ...]
```

This is a conditional repair command, **not an upload performed in this
inspection**. Never substitute `bin9208/openpilot` or `ajouatom/openpilot`,
and do not use a blanket historical `--all` upload or change pointer sources.

## Follow-up: reset target versus LFS endpoint

At 2026-10-21 00:35:51 KST, unauthenticated **download** batch requests for
the reported OID and 6,166,229-byte size returned:

| Repository LFS endpoint | HTTP status | Per-object result |
| --- | ---: | --- |
| `https://github.com/bin9208/openpilot-rust.git/info/lfs` | 200 | Download action present; no error |
| `https://github.com/bin9208/openpilot.git/info/lfs` | 200 | Error code 404, object absent |
| `https://github.com/ajouatom/openpilot.git/info/lfs` | 200 | Error code 404, object absent |

These are confirmed current endpoint differences. Requests used
`POST <endpoint>/objects/batch` with this exact JSON body:

```json
{"operation":"download","transfers":["basic"],"objects":[{"oid":"5d20cdbb457ba18db51a537ee2e305bbe442264b1613956068d473e35d15900d","size":6166229}]}
```

Headers were `Accept: application/vnd.git-lfs+json` and
`Content-Type: application/vnd.git-lfs+json`; no authentication was sent.
No upload or repository mutation occurred. The private archive's
`endpoint-comparison.json` records the sanitized responses. A batch HTTP 200
does not mean its individual objects exist.

The installed Git LFS is **3.4.1**. Local `git lfs help fetch`,
`git lfs help config` and `git lfs help smudge` were read. Version-matched
primary source confirms these distinctions:

- An explicit `git lfs fetch rust <ref>` selects `rust` for that fetch;
  the source sets the remote from its first argument.
  [Fetch source](https://github.com/git-lfs/git-lfs/blob/v3.4.1/commands/command_fetch.go#L35-L58).
- Smudge/filter-process uses `cfg.Remote()`. Remote autodetection is
  optional and defaults off; the filter only derives a remote from the
  treeish when that option is enabled.
  [Smudge source](https://github.com/git-lfs/git-lfs/blob/v3.4.1/commands/command_smudge.go#L109-L119),
  [filter-process source](https://github.com/git-lfs/git-lfs/blob/v3.4.1/commands/command_filter_process.go#L87-L104).
- The default selection order is the current branch's `branch.<name>.remote`,
  then `remote.lfsdefault`, then the sole configured remote, then `origin`.
  Resetting to a remote-tracking ref does not set branch tracking configuration.
  Thus an existing original checkout can still choose its original endpoint
  after `git reset --hard refs/remotes/rust/dev-md`.
  [Remote selection source](https://github.com/git-lfs/git-lfs/blob/v3.4.1/config/config.go#L214-L240).
- A configured `lfs.url` takes precedence over remote-specific endpoint
  derivation, including when an explicit fetch remote is supplied. Without
  that override, `remote.<name>.lfsurl` or the selected remote's clone URL
  determines the endpoint. Naming `rust` is useful but cannot override an
  already configured repository-wide `lfs.url`.
  [Endpoint source](https://github.com/git-lfs/git-lfs/blob/v3.4.1/lfsapi/endpoint_finder.go#L72-L117).

**Inference:** the tester's reset likely selected the original repository's
LFS endpoint, where the exact OID currently returns 404. The endpoint
comparison and remote-selection mechanism support this explanation; they
do not confirm the tester's actual request URL, Git LFS version or effective
configuration. Confirmation requires the tester's `git lfs logs last`,
`git lfs env`, current branch/upstream and relevant effective configuration.
Inspect those locally, redacting credentials before sharing logs:

```bash
git lfs version
git branch --show-current
git config --show-origin --get-regexp '^(branch\..*\.remote|remote\..*\.(url|lfsurl)|remote\.lfsdefault|lfs\.url|lfs\.remote\.(autodetect|searchall))$'
git lfs env
git lfs logs last
```

The earlier independent-repository audit proves availability there, not that
reset in a differently configured original clone would use that endpoint.

## Tester recovery after the reported reset failure

Run these commands in the tester's existing checkout. First inspect
`git remote get-url rust`: `rust` must resolve to `bin9208/openpilot-rust`.
Keep the original `origin` remote unchanged. The **one-time command-line
`lfs.url` override** below pins the independent endpoint even if the current
branch tracks `origin` or an existing Git configuration points LFS elsewhere.

```bash
git remote get-url rust
git lfs env
git status --short
git fetch --no-tags rust refs/heads/dev-md:refs/remotes/rust/dev-md
git -c lfs.url=https://github.com/bin9208/openpilot-rust.git/info/lfs \
  -c lfs.fetchrecentalways=false -c lfs.skipdownloaderrors=false \
  -c lfs.remote.autodetect=false -c lfs.remote.searchall=false \
  lfs fetch --include='' --exclude='' rust refs/remotes/rust/dev-md
```

Stop if fetching fails. Fetching first obtains all required LFS objects in
the checkout's normal local LFS store before another reset is attempted.
Preserve any desired local work before retrying the tester's intended reset:
`--hard` discards tracked local changes. Apply the same endpoint override to
the reset so any filter download also uses the independent repository:

```bash
git -c lfs.url=https://github.com/bin9208/openpilot-rust.git/info/lfs \
  -c lfs.skipdownloaderrors=false \
  reset --hard refs/remotes/rust/dev-md
```

Once HEAD is the intended commit, explicitly materialize the assets and
inspect their integrity:

```bash
git lfs checkout
git -c lfs.fetchexclude='' lfs fsck --objects HEAD
sha256sum openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx
sha256sum openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx
wc -c openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx
```

For the audited source, the two hashes/sizes must match the table above.
`fsck --objects` checks the local LFS object store; the explicit hashes and
byte counts check the materialized model files. A changed/modified asset
must not be silently accepted: `git lfs checkout` preserves modified files,
so inspect mismatches before proceeding. If dev-md has advanced, compare
against that commit's pointers rather than assuming this inventory is current.

Do not set permanent `GIT_LFS_SKIP_SMUDGE`, install `--skip-smudge` filters,
enable `lfs.skipdownloaderrors`, or accept a pointer file as a model.
If a previous workaround already saved skip-smudge filters, restore the
normal repository filters with `git lfs install --local --force --skip-repo`
(preserving existing hooks) and clear the shell's
`GIT_LFS_SKIP_SMUDGE` setting before future ordinary checkouts. None of these
tester recovery commands were executed on a device by this task.

## Minimal durable endpoint pin — implemented; cloud verification pending

The originally audited `origin/dev-md` tree at `a2b06522` had **no
`.lfsconfig`** (confirmed with `git show origin/dev-md:.lfsconfig`). After
the endpoint investigation, the user authorized the minimal branch-owned
fix. The shared worktree now contains a root `.lfsconfig` with only:

```ini
[lfs]
    url = https://github.com/bin9208/openpilot-rust.git/info/lfs
```

`lfs.url` is an allowed `.lfsconfig` key. This makes endpoint selection
independent of the clone's remote names or the current branch's original
upstream while that branch's configuration is effective. It also supplies
the default LFS upload endpoint; it does not change Git remotes, authorize
source pushes, or authorize uploads to either original repository.
Explicit push URL settings can override upload routing and must still be
checked before any future authorized upload.

The pin is preferable to a permanent `git config --local lfs.url ...` in an
original clone, because a local setting persists when switching back to an
original branch. A tracked branch-owned file follows that branch's contents.
However, ordinary Git configuration **overrides `.lfsconfig`**, and LFS reads
the working-tree file first, then the index, then HEAD. A filter process
already started during the initial transition can therefore see the old
configuration; the tracked file is not a substitute for the explicit
endpoint-pinned prefetch/reset above on first migration. After switching,
inspect `git lfs env` and effective overrides rather than assuming a pin won.
These configuration rules are documented in
[Git LFS 3.4.1 configuration](https://github.com/git-lfs/git-lfs/blob/v3.4.1/docs/man/git-lfs-config.adoc#configuration-files)
and implemented by
[configuration loading](https://github.com/git-lfs/git-lfs/blob/v3.4.1/git/config.go#L142-L170).

`.lfsconfig` and `tools/ci/tests/test_lfs_endpoint.py` were added alongside
this document under the expanded user authorization. Clone-local Git configuration,
branch tracking and pointer sources were not changed. No commit or source
push was made by this task. The parent owns workflow changes and cloud
dispatch; this task did not edit CI workflow files.

## Real Git/LFS fixture regression — cloud execution only

`tools/ci/tests/test_lfs_endpoint.py` uses only Python standard-library
`unittest` and helpers, installed `git`/`git-lfs`, temporary repositories and
a loopback HTTP LFS batch/download server. It does not use GitHub credentials,
upload objects, download real models, install packages or alter the shared
worktree. The production `.lfsconfig` is consumed with only the authorized
endpoint translated to the fixture's loopback service address. Removing the
pin or changing it to an original repository must fail the pinned-reset case.

Three cases exercise real subprocess/filter/network behavior:

1. Starting from an original branch tracking `origin`, with two remotes and
   an empty cache, reset to `refs/remotes/rust/dev-md` containing the tracked
   pin must materialize the exact synthetic model bytes and SHA-256 from
   the correct endpoint. The current branch and its `origin` tracking stay
   unchanged; no repository-local `lfs.url` is saved.
2. Without the pin, the original endpoint returns a batch object error 404
   and reset must fail. After restoring the fixture's original commit,
   explicit endpoint-pinned `lfs fetch rust refs/remotes/rust/dev-md` must
   populate its cache while leaving HEAD/worktree on the original commit.
   The subsequent reset must materialize the verified bytes from that cache.
3. A stale repository-local `lfs.url` must defeat the tracked pin, and naming
   `rust` alone must also fail. The same one-time `-c lfs.url=...` fetch/reset
   must recover while preserving the stale local value, original Git remote
   and branch tracking. Normal required LFS filters remain enabled, with no
   saved skip-smudge or skip-download-errors workaround.

Inherited Git configuration and workflow skip-smudge environment settings
are excluded from the fixture. The test class runs only when
`GITHUB_ACTIONS=true` and `RUNNER_ENVIRONMENT=github-hosted`; ordinary local or
self-hosted discovery skips it. Missing Git LFS on a hosted runner fails
the test rather than reporting a skipped success.

Hosted command ready for the parent-owned manual workflow:

```bash
python -m unittest discover -s tools/ci/tests -p 'test_lfs_endpoint.py' -v
```

**Execution status:** not run locally, as explicitly required. Local review
is limited to Python syntax, Git configuration parsing and diff/whitespace.
No red/green functional result or exact-SHA cloud pass is claimed; the parent
must record the hosted Actions run and resolve any failure before acceptance.

## Future CI integration proposal — coordination required before edits

No CI files were edited. Existing `.github/workflows/rust.yml` downloads
only selected models in `xiaoge-runtime`, `xiaoge-memory` and model pipeline
jobs. Those pulls use the ordinary LFS store; they do not prove that every
tracked pointer can be downloaded from an empty store. Existing
`.github/workflows/dev-md-runtime.yml` also explicitly skips checkout smudging.
A successful build or cached model pull therefore cannot stand in for a
complete checkout availability gate.

Proposed integration into the **existing** workflows, for parent coordination:

1. Add a small availability job to `fast-checks.yml`, explicitly on
   `ubuntu-24.04`. Inspect the event's exact source tree, enumerate canonical
   pointer paths/OIDs/sizes, and request download batch metadata from the
   independent repository. Fail on missing objects, HTTP errors, absent
   download actions or size disagreement; do not let an HTTP-200 batch with
   per-object 404 errors pass. Make the existing required `fast checks` job
   depend on and require this job's success. Keep this branch check bounded;
   it checks availability metadata, not complete payload integrity.
2. Add an `lfs-integrity` job to `rust.yml`, explicitly on `ubuntu-24.04`,
   for PR/deep integration checks, dev-md pushes and manual runs. Use
   `actions/checkout` with `lfs: false`, a unique empty
   `$RUNNER_TEMP` LFS storage directory, no LFS cache restore or alternate
   repository fallback, and read-only access to the independent repository.
   Override recent fetching and include/exclude filters, set
   `lfs.skipdownloaderrors=false`, fetch every pointer at the exact checked
   out SHA, and verify each object's SHA-256 and byte count. Also verify
   pointer/attribute consistency so an accidentally untracked pointer cannot
   disappear from the inventory. Record the source SHA and concise per-object
   result without tokens or signed URLs. Clean temporary objects on exit.
3. Wire that job into the existing `rust checks` aggregate's `needs` and
   success checks; an unrequired standalone success is insufficient. Require
   its exact-source success before runtime publication. Existing focused
   model jobs remain useful for behavior checks and should keep their scope.
4. Validate the proposed gates in GitHub Actions, including a synthetic
   missing-OID failure case. Do not reproduce functional checks locally or
   route the new integrity job onto the local/self-hosted runner. Keep
   functional/ARM/package checks separate from quick metadata checks.

CI implementation, exact-SHA Actions evidence and a tester retry have not
been claimed by this investigation. The issue remains open for coordination
and the parent task's acceptance decision.
