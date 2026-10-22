# Issue #292 — runtime SDK directory aliases

Issue: https://github.com/bin9208/openpilot-rust/issues/292. Coordinated batch:
#497, `codex/fix-497-stale-runtime-defects`, base
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`. No commit/publication by this worker.

## Ownership and cause

Read-only inspection of #296/PR343 and the local
`codex/feat-296-repository-python-retirement` committed tree found
`rust/tools/extract_runtime_sdk.py` still present, with its original Git blob
`007cc4f723139d3a15fd986a76fb5102f3dc9131` listed as pending in that track's
inventory. No native SDK extractor replacement was found. This patch repairs the
existing Python tool; it does not port it, change callers or claim retirement
credit. Other sessions' checkouts and uncommitted files were not accessed.

The tree walker classified only directory-mode ls entries as directories. A
symlink entry went to file extraction; following it yielded a directory, which
was rejected as a missing image file.

## Defined behavior

Resolve each image entry before deciding whether to extract a regular file or
walk a directory. Materialize each resolved image directory once at its
canonical image path under the SDK output root. Additional paths to it become
relative symlinks to that canonical materialization, contained within the
selected SDK output root. This also permits an alias to a
directory elsewhere in the verified image: its contents are staged under the
requested output path, never followed into the host filesystem. Alias ordering
does not affect accessible header bytes. Repeated/overlapping tree requests
reuse completed traversal. A child-first request can create a canonical parent
before that parent is visited; its later traversal adopts the existing directory
and retains the already extracted children. Directory-alias file rows therefore
refer to canonical output paths, with requested aliases recorded separately;
ordinary file extraction/provenance is unchanged.

Directory ancestor cycles fail explicitly. File/link resolution is limited to
64 entries, directory depth to 128 and distinct directories to 100,000. Image
paths must be absolute and cannot inject debugfs quoting/control characters.
Output symlinks cannot escape the destination; the output root itself cannot be
a symlink. Existing disk reserve, exact image size/SHA256, missing-file rejection,
regular-file dump-size check and per-file SHA256 remain mandatory. Ordinary
file alias chains are retained. No SDK version, provider, license or source
identity changes.

The record preserves `image`, `raw_sha256` and the existing `files` rows, and adds
`directory_aliases` containing requested/resolved image identity, link chain,
output path and contained target for output directory aliases. Headers under an
alias are accessible at every requested path without duplicate byte extraction.
Cycle/depth violations fail rather than silently omitting content.

Tests were written first and invoke the real CLI/debugfs on owned 16 MiB ext4
images made with `mke2fs -d`. Cases cover relative/absolute/chained aliases,
repeated/overlapping roots in parent-first and all six child/alias/parent request
permutations, retained file aliases/hashes, aliases outside the
selected image subtree, cycle and excessive-chain rejection, missing and dangling
entries, output containment and image identity failures. No large AGNOS image,
physical device or installation is required for these regressions.

## Hosted verification recipe (not executed locally)

Use GitHub-hosted Ubuntu with Python 3.11+ and e2fsprogs, retaining at least the
existing 27 GiB extraction reserve. Run from repository root. The parent retains
candidate tests while restoring the original production file, handles expected
RED nonzero exit, and restores the candidate even on failure in the disposable
Actions checkout.

```bash
cp rust/tools/extract_runtime_sdk.py "$RUNNER_TEMP/extract-runtime-sdk-candidate.py"
git show b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a:rust/tools/extract_runtime_sdk.py > rust/tools/extract_runtime_sdk.py
python3 -m unittest discover -s rust/tools/tests -p test_extract_runtime_sdk.py -k directory_aliases_preserve_headers_without_duplicate_tree_traversal -v
# Expected RED: the original extractor rejects a directory reached via symlink.
cp "$RUNNER_TEMP/extract-runtime-sdk-candidate.py" rust/tools/extract_runtime_sdk.py
python3 -m unittest discover -s rust/tools/tests -p test_extract_runtime_sdk.py -v
```

Local Python syntax and scoped diff review pass. Owned-image RED/GREEN await
the parent's exact-source hosted results. An actual pinned AGNOS `/usr/include`
tree was not extracted in this task; the focused owned-image regression is the
issue's requested debugfs/image boundary check. It does not claim a full AGNOS
SDK, native replacement or workstation/device installation result.

## Harvey P2 follow-up — child-first roots

The first candidate had an ordering bug: `/usr/include/real/nested`, then
`/usr/include/relative` (image link `relative -> real`), then `/usr/include/real`
created `real` implicitly, registered `relative` as the real parent's
materialization, then attempted to replace the existing `real` directory with a
symlink. It failed with `FileExistsError`; simply adding reciprocal links would
also risk a `real/nested` ↔ `relative/nested` cycle.

The owned-ext4 regression was added before this follow-up implementation. It
runs that exact order first, all six permutations and a repetition of each full
request sequence. It requires a real canonical directory, a relative alias to
it, preserved child/header bytes and hashes, exactly one extraction record per
file, and a single noncyclic directory alias. Existing limits and negative cases
are retained. No local fixture or test was run.

Parent must additionally record hosted RED against the first candidate and
GREEN against this follow-up, using the retained first-candidate tool with the
new tests for RED. The focused hosted command is:

```bash
python3 -m unittest discover -s rust/tools/tests -p test_extract_runtime_sdk.py -k child_first_overlapping_roots_and_all_permutations_preserve_materialized_files -v
```

Full owned-image suite acceptance remains pending the parent's hosted run. Only
the extractor, its exclusive tests and this issue record changed in this
follow-up; no commit, other worker patch or original checkout was touched.
