# #503: native APT in the indirect setup caller

This patch is prepared on backlog source
`2765b4ab1c15a05c72d23c4ca32c033011c14543`. It changes only
`tools/setup_dependencies.sh`, adds its focused regression tests, and records
this evidence. It does not import the common branch's history, PRoot changes,
runtime changes, or the separate #296 tool conversion.

## Matching source and observed failure

Before this patch, the setup script's Git blob was
`ec9f99d701c493ea278a4b59267a362953c641c3`. That is also the blob at common
failed source `d27cd41ee8082cd7feb76f8ec59c8d7a9701d37b` and the parent of
`77663836a95a9e2aff9aa66592d8b06ed9c87cd9`. The two implementation/test files
are copied byte-for-byte from that reviewed common fix, without merging it.

The backlog caller is `.github/workflows/tests.yaml`'s `build_release` job:
`./tools/op.sh setup` invokes `tools/setup_dependencies.sh` in `op_setup`.
Wrapping direct workflow APT calls does not cover this script's own APT calls.
The old script also queried Ubuntu package status without a native-architecture
qualifier, allowing a foreign-only installation to satisfy that check.

[Common CI run 39850289766, build release job 120079846026](https://github.com/bin9208/openpilot-rust/actions/runs/39850289766/job/120079846026)
ran the same `./tools/op.sh setup` caller at `d27cd41ee`. Its log identifies
Ubuntu Noble, then reports HTTP 404 for `binary-arm64/Packages` on
`archive.ubuntu.com` for `noble`, `noble-updates`, and `noble-security`.
Dependency setup failed and the step exited 1 before the build. This is actual
failure evidence for the matching old script, not a backlog regression result.
The log does not establish whether the foreign defaults came from dpkg
registration or APT configuration, and does not demonstrate the separate
foreign-only package-status defect.

## Minimal change and boundaries

The script obtains the native package architecture with
`dpkg --print-architecture` only when APT is available. Ubuntu package-status
checks use `package:native-architecture`. Its two APT invocations set the scalar
`-o APT::Architectures=native-architecture`, replacing inherited architecture
defaults rather than appending a list entry with `::`.

All package names, install options, manual setup entry points, non-Actions
execution, other package managers, Python dependency setup, and udev handling
are preserved. This caller uses APT directly and does not introduce a dependency
on the Actions-only wrapper. It does not add/remove dpkg architectures, edit
shared APT sources/configuration, or change explicit cross-build source lists.
Explicit per-source architecture options remain caller policy; this default
override is not a source-list sanitizer.

## Regression coverage and next Actions execution

`tools/ci/tests/test_setup_dependencies.py` exercises the actual
`install_linux_deps` function with fake package commands and guarded udev
operations. Its four test methods cover amd64/arm64 native contexts with
dpkg/list/scalar inheritance, the complete package argument list, foreign-only
package status, skipping APT for installed native packages, and update failure
stopping before install. Foreign registration is kept unchanged by the fixture.
These are function-boundary regressions, not a real package-install or device
acceptance result.

The existing `.github/workflows/fast-checks.yml` unittest discovery already
registers the new file. Run the focused recipe only in Actions:

```sh
python -m unittest discover -s tools/ci/tests -p test_setup_dependencies.py -v
```

The next necessary combined source validation must also exercise the real
`./tools/op.sh setup` caller and retain the normal required checks. All Actions
jobs use registered `bin-actions`; existing ARM checks use the separately owned
#502 ARM QEMU/PRoot environment and must be reported as emulated execution.
There is no hosted-runner fallback or routing-only repeat of unchanged success.
Preserve current backlog CI while the parent reviews and batches the necessary
indirect-APT and persistent-template-cleanup fixes.

Local checks for this patch are limited to Python AST parsing, Bash/YAML syntax
and diff inspection. No local functional tests, project imports, builds, CI
dispatch, commits, pushes or GitHub mutations were performed. Exact backlog
candidate Actions results remain pending; this patch does not claim updater,
AGNOS, installer, or device acceptance.
