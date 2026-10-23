# #512: publish readable main web evidence without changing Params

Issue: [#512](https://github.com/bin9208/openpilot-rust/issues/512).
Preparation source: `b1db2b0f839a3a2e5c2f8ba8317d97b4b03faccb`;
[PR #500](https://github.com/bin9208/openpilot-rust/pull/500) targets dev.
Status: parent workflow change and sidecar tests prepared; actual Actions
permission tests and artifact publication remain pending. No local fixture,
sudo, test, project import, dependency installation or build was executed.

## Actual failure and scope

The parent reports actual Focused web job **120189124011** at source b1db
passed its functional, source and lint checks, including main server termination,
supervisor termination and forced termination. Its only failure was
`actions/upload-artifact@v6`: `EACCES` opening root-created
`web-main/force/params/.../CarrotNaviHudMapProfile`. No evidence artifact was
published. The owned job-temp cleanup's root fallback succeeded. The primary
capture is `web-b1db-fail.log` in the parent's analysis scratch directory;
this sidecar does not claim an independent reread of that capture.

The main process probes run in a privileged namespace and create private Params
as root. The upload action reads paths as the runner user. Uploading the three
raw case trees crossed that permission boundary. Successful functional steps
therefore did not produce a successful evidence handoff. This is an evidence
packaging defect; it does not establish another production terminal failure.

## Archive boundary

The parent owns `.github/workflows/backlog-fixes-497.yml`. Its settled step,
`Archive owned main web evidence for the runner`, runs on `always()` for web,
after the probes and before upload. It accepts only a direct, non-symlink
`/tmp/openpilot-ci.*` directory with a regular, non-symlink `owner` marker
matching `GITHUB_RUN_ID:GITHUB_RUN_ATTEMPT:GITHUB_JOB`. It rejects an indirect
`web-main` root and existing archive or hash outputs, including symlinks.

The runner creates a private 0600 temporary archive. Sudo reads only existing
`term`, `supervisor`, `force` case directories, selecting them inside the
privileged block so a root-owned 0700 public directory works. A case symlink or
non-directory fails. `find -P` supplies only directories and regular files;
tar uses null-delimited, verbatim names and disables recursive traversal.
Symlinks, FIFOs and sockets are excluded. Zero cases supplies an empty file list
and creates a valid empty archive, without find's implicit `.` traversal.
The parent TLS key/certificate and unrelated external paths are not selected.

Root writes the already runner-owned temporary file. No chown or chmod of Params
or evidence is needed; tar retains each source member's uid/gid/mode. Exclusive
hard-link publication produces `web-main-evidence.tar.gz`, followed by the
noclobber `web-main-evidence.sha256`. Upload names these two files instead of the
three raw trees. The source capture and Python-binding provenance paths remain
registered. Existing source/functional probes and their deadlines are unchanged.

## Five actual-script regressions

`tools/ci/tests/test_web_main_artifact.py` parses the real workflow through
PyYAML and executes that exact step's run script, without copying its archiver.
Tests require an unprivileged Actions user and real passwordless sudo; no mocks
or skips substitute for the permission boundary. Each fixture is an explicit
`TemporaryDirectory(prefix='openpilot-ci.', dir='/tmp')`, independent of the
outer job's TMPDIR. Bounded privileged setup/metadata/cleanup verifies the
original fixture's direct `/tmp` path, non-symlink directory, device, inode and
caller uid. Cleanup never takes its path from a negative test's CI_JOB_TEMP.

The five cases cover:

1. All three root-owned 0600 Params under root-owned 0700 directories: direct
   runner reads must fail, but archive reads must succeed. Verify exact members,
   content, caller archive ownership/mode, source uid/gid/mode/mtime retention
   and the complete SHA-256 manifest.
2. External/parent key links, a directory link, dangling link, FIFO and Unix
   socket: only the regular case content reaches the archive, within a bounded
   invocation. A regular filename containing a newline exercises null-delimited
   collection. Verify private source metadata remains intact.
3. Private parent key/certificate with no cases: the archive has no members.
4. Wrong owner marker, symlinked owner marker, nested CI_JOB_TEMP, symlinked
   public root, symlinked case and regular-file case: reject without publishing
   archive/hash or leaving the owned temporary archive behind.
5. An existing archive or manifest, both regular files and symlinks: reject and
   preserve destination identity/bytes and symlink target, with no other output.

The four existing `test_backlog_web_registration.py` tests retain their
report/live/terminal registrations and assertions. Their ordering check now also
requires the archive step's web/always condition and placement, the two archive
paths as the complete main-web upload selection, and continued source/binding
evidence before final owned cleanup.

## Registration and pending acceptance

The existing Fast checks job already installs PyYAML and discovers both files:

```sh
python -m unittest discover -s tools/ci/tests -v
```

For a focused Actions diagnostic, from the repository root:

```sh
python -m unittest discover -s tools/ci/tests -p 'test_web_main_artifact.py' -v
python -m unittest discover -s tools/ci/tests -p 'test_backlog_web_registration.py' -v
```

All functional execution belongs in registered Actions on `bin-actions`
(`self-hosted`, `Linux`, `X64`, `dev-md-local`), with no GitHub-hosted fallback.
Local checks are Python AST/embedded-fixture syntax, YAML inspection and diff
review only. This sidecar changes only those two test files and this record;
the parent owns workflow/public metadata and any dispatch/commit/publication.
The preparation check parsed both Python files and six embedded fixture snippets,
loaded the settled workflow YAML, and passed `git diff --check`. It counted five
functional and four registration test methods without executing them.

Before closure, retain the exact candidate SHA and actual Actions results for
the five real-sudo cases and all four registration tests. The next necessary
Focused web run must retain its complete functional/source/lint acceptance,
show successful archive creation, upload and owned cleanup, and publish a
downloadable artifact with the archive and matching hash. Inspect the archive
members to confirm all three public case trees and their required receipts are
present, source ownership/modes remain private, and TLS keys and links/special
files are absent. Unit success alone cannot prove upload-artifact publication.
Normal exact-head full CI, PR review/merge and post-merge gates remain required;
this packaging fix waives no hardware, device or historical acceptance limit.
