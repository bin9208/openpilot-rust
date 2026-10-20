# dev-md fork checkout integrity — issue #415

Issue: [#415](https://github.com/bin9208/openpilot-rust/issues/415).
Audit: October 21, 2026, 00:30–00:31 KST. Source:
`a2b06522a24f4a3533192daf99ac48f119782bda` (`origin/dev-md` at inspection).

## Diagnosis

The tester's `git reset --hard refs/remotes/rust/dev-md` fails with an LFS
smudge 404 for `v_asm_model.onnx`, exact SHA-256
`5d20cdbb457ba18db51a537ee2e305bbe442264b1613956068d473e35d15900d`,
6,166,229 bytes. A remote ref passed to `git reset` selects source contents;
it does not select the LFS endpoint or change the current branch/upstream.
An original clone can therefore still ask its original `origin` for models.

Read-only LFS batch requests for this exact object return:

| Endpoint | HTTP status | Object result |
| --- | ---: | --- |
| `bin9208/openpilot-rust.git/info/lfs` | 200 | Download available; downloaded hash/size verified |
| `bin9208/openpilot.git/info/lfs` | 200 | Object error 404 |
| `ajouatom/openpilot.git/info/lfs` | 200 | Object error 404 |

This reproduces the error mechanism. The tester's effective Git/LFS
configuration has not been supplied, so the exact endpoint used on that
checkout remains unconfirmed. No objects were uploaded and no original
repository was modified.

All **seven** current dev-md LFS objects were independently downloaded into
initially empty LFS storage from the independent repository, with remote
search/autodetection disabled. Every SHA-256 and byte count matches its
committed pointer. Total: **104,864,948 bytes**; missing objects: **zero**.
Available local space was 118,008,430,592 bytes, above 25 GiB plus the
331,372,060-byte growth estimate. No builds or dependency installs occurred.

| Tracked path | SHA-256 / LFS OID | Bytes | Primary local cache | Independent download | Uploaded |
| --- | --- | ---: | --- | --- | --- |
| `docs/assets/comma-logo.png` | `f54dec485ad298ee17b533354ed6f1fdfd3711a163d2c19d758f2ae340a0276e` | 57 | Absent | Verified | No |
| `openpilot/selfdrive/carrot/web/assets/alert_camera.svg` | `f1086279a32f49f68de1db4e89a93aa2aae779395cfb908e5f6a0cbf2bf7dae6` | 989 | Verified | Verified | No |
| `openpilot/selfdrive/carrot/web/assets/alert_police.svg` | `d0dea6745d5cd5cf8206ced256310fed587555b3309d80cad46a4c066a931401` | 707 | Verified | Verified | No |
| `openpilot/selfdrive/carrot/xiaoge/assets/lane.onnx` | `d3761c185daf33897ff3ff5edf28115c5dc829c06c538033accb616d6c67528c` | 13,194,895 | Verified | Verified | No |
| `openpilot/selfdrive/carrot/xiaoge/assets/v_asm_model.onnx` | `5d20cdbb457ba18db51a537ee2e305bbe442264b1613956068d473e35d15900d` | 6,166,229 | Verified | Verified | No |
| `openpilot/selfdrive/modeld/models/driving_supercombo.onnx` | `f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5` | 60,792,584 | Verified | Verified | No |
| `openpilot/system/hardware/tici/updater` | `273785b6fa93585f1a6f7bff25f253abdafafdcdbba25c4369db6c52a0f00daa` | 24,709,487 | Absent | Verified | No |

For each path, the audit read `git show <source>:<path>` and compared the
pointer's OID/size against both the existing local cache, if present, and a
fresh `git -c lfs.storage=<empty-storage> -c lfs.fetchrecentalways=false
-c lfs.remote.autodetect=false -c lfs.remote.searchall=false lfs fetch
--include=<path> --exclude='' origin <source>` download. No signed URLs or
credentials were retained. These are asset-integrity checks, not model or
vehicle functional validation.

## Recovery in the existing tester clone

Verify `git remote get-url rust` identifies `bin9208/openpilot-rust`.
The following fetch explicitly selects its LFS endpoint, overriding a stale
local `lfs.url`. It must succeed before switching. The switch discards tracked
local edits, as did the tester's requested hard reset; preserve wanted edits
first. Keep the original `origin` remote unchanged.

```bash
git fetch --no-tags rust refs/heads/dev-md:refs/remotes/rust/dev-md
git -c lfs.url=https://github.com/bin9208/openpilot-rust.git/info/lfs \
  -c lfs.fetchrecentalways=false -c lfs.skipdownloaderrors=false \
  -c lfs.remote.autodetect=false -c lfs.remote.searchall=false \
  lfs fetch --include='' --exclude='' rust refs/remotes/rust/dev-md
git -c lfs.url=https://github.com/bin9208/openpilot-rust.git/info/lfs \
  switch --discard-changes -C dev-md refs/remotes/rust/dev-md
git branch --set-upstream-to=rust/dev-md dev-md
git lfs checkout
git -c lfs.fetchexclude='' lfs fsck --objects HEAD
```

The switch selects a real `dev-md` branch and tracks `rust/dev-md`; a hard
reset alone would leave the previous branch name/upstream in place. For the
audited source, materialized model SHA-256/size must match the table. If dev-md
advances, compare against that commit's pointers. No tester command was
executed on a device by this task; actual tester retry remains outstanding.

Do not leave permanent skip-smudge filters or `lfs.skipdownloaderrors` enabled,
or treat LFS pointer text as an ONNX model. Inspect `git lfs env` and
`git config --show-origin --get-regexp '^(lfs\.url|remote\..*\.lfsurl)$'`
for remaining explicit overrides; redact credentials before sharing output.

## Durable fix and precedence

The branch adds only this root `.lfsconfig` setting:

```ini
[lfs]
    url = https://github.com/bin9208/openpilot-rust.git/info/lfs
```

It selects the independent repository even in clones whose remote names or
previous upstream differ. Explicit Git configuration overrides the tracked
file, and an already running filter can have read the previous configuration;
therefore first migration uses the explicit endpoint-pinned prefetch above.
A tracked pin follows branch contents, unlike a permanent clone-local override
that would also affect switching back to an original branch. The setting also
supplies the default LFS upload endpoint; it does not authorize any upload.

See [Git LFS configuration precedence](https://github.com/git-lfs/git-lfs/blob/v3.4.1/docs/man/git-lfs-config.adoc#configuration-files)
and [endpoint selection](https://github.com/git-lfs/git-lfs/blob/v3.4.1/lfsapi/endpoint_finder.go#L72-L117).
No model bytes, pointer OIDs, Git remotes or existing branch refs are changed
by this source patch.

## Cloud regression evidence

`tools/ci/tests/test_lfs_endpoint.py` runs real Git/LFS commands against an
isolated loopback batch/download server with synthetic model bytes. It clears
inherited skip-smudge/configuration and verifies actual materialized bytes,
SHA-256, local object cache, retained original remote/upstream and required
smudge filtering. Missing git-lfs is a failure, not a skipped regression.

The first three cases passed on GitHub-hosted job
[118728550325](https://github.com/bin9208/openpilot-rust/actions/runs/39451883186/job/118728550325),
source `7f32501862f30606417fd1eb684bfdec1438547e`:

- Tracked pin downloads despite the original branch's upstream.
- Unpinned original endpoint fails 404, then forced prefetch/reset recovers.
- A stale local URL overrides the pin; the command-line URL recovers it.

An additional case switches to and tracks `dev-md` while preserving the old
branch tip. Final exact-source PR verification remains pending. Local checks
are syntax, formatting and diff review only; no local functional test, build,
installation, sealing or publication was performed.
