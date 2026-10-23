# #506: preserve action Python library paths around native providers

[Issue #506](https://github.com/bin9208/openpilot-rust/issues/506) follows the
actual x64 webcam failure at source
`2765b4ab1c15a05c72d23c4ca32c033011c14543`,
[Actions job 120091333543](https://github.com/bin9208/openpilot-rust/actions/runs/39855085643/job/120091333543).
This candidate starts at local `c67fb9025ba00ae4dfd6e68931c2f73e12636041` and
preserves its six already-committed APT/template fixes. No push, dispatch,
cancellation, commit, dependency installation or functional execution was
performed for this preparation. Parent review and exact-candidate Actions
results remain pending.

## Observed failure and causal limit

The job log records `av-16.1.0` successfully installed (line 401). Before the
provider export, `LD_LIBRARY_PATH` includes
`/home/actions/actions-toolcache/Python/3.12.14/x64/lib` (lines 303 and 435).
The literal export at line 414 replaces it with only the private OpenCV
provider directory; subsequent steps show that provider-only value (664 and
939). `webcam_camera_compare.py` then fails at `import av` with
`ModuleNotFoundError` (945). `pythonLocation` still names the action toolcache;
that environment variable does not establish the interpreter's actual prefix.

The environment erasure is verified. A different loaded libpython causing a
different prefix/site-packages search is a strong source-based inference,
pending actual before/old/candidate receipts. The pinned
[setup-python implementation](https://github.com/actions/setup-python/blob/ece7cb06caefa5fff74198d8649806c4678c61a1/src/find-python.ts)
prepends its library directory to the inherited path. CPython's
[prefix search](https://github.com/python/cpython/blob/v3.12.14/Modules/getpath.py)
can use the runtime library location and compiled prefix. No Python prefix,
architecture, version or import result is fabricated to demonstrate this.

## Scoped change

The webcam and Xiaoge provider exports in `.github/workflows/rust.yml` keep the
provider first and append the existing **nonempty** `LD_LIBRARY_PATH` tail
verbatim. Unset/empty tails produce no extra colon. Existing FFmpeg/OpenCV and
Python package pins, all original build/source/pixel/capture/codec/IPC commands,
timeouts, source checks, matrix values, trust guards and required gates remain.
WebRTC has no matching overwrite and is unchanged.

The same composition applies to the Python source children of
`check_xiaoge_runtime.py` and `check_xiaoge_lifecycle.py`. Both Python wrappers
in `check_xiaoge_live.py` need the inherited tail; its native Rust child gets
the original provider-only value explicitly. Native runtime/lifecycle children
and the separate sanitizer environment retain their isolation. Native process
map and executable-hash assertions remain intact. These changes do not alter
vehicle runtime behavior, source matching, model outputs or deadlines.

The small stdlib helper `tools/ci/provider_python_env.py` supplies composition
and receipts. It imports no project runtime. Its job-specific smoke modules
come from the already-pinned closures: webcam checks `av`, `cv2`, `numpy`,
`capnp`; Xiaoge checks `cv2`, `numpy`, `capnp`, `httpx2`, `zmq`. It installs
nothing. Full existing source comparisons remain the wider validation.

## Controlled baseline provenance

The old fragments are exact stripped lines from immutable commit `2765b4ab...`,
Git blob `55170bb2f75746d668d7e3c2ded1f4b197f4a222` at
`.github/workflows/rust.yml`. SHA-256 of each fragment, excluding its newline:

| Fragment | SHA-256 |
| --- | --- |
| Webcam | `af82621374538fa9bd551b5b8d3a9bc98d47717964f465ff55eef068bef00bd8` |
| Xiaoge | `ef4da3c6390f36795f9ef8edd7500c0e407cacef68abf540c8771266ca9d0b13` |

The fast regression reads the pinned Git object, authenticates the whole blob,
and compares both actual lines with the stored fragments. Fast checks already
fetch full history; no additional fetch/network path is introduced. The real
job probe verifies the fragment hash and expands it in a separate Bash process
with a private command file, then launches the real current `sys.executable`
under those exported values. It never applies that old environment to the
candidate step. No downloaded code or synthetic interpreter is used.

## Actions verification and receipts

Tests were written before the implementation. Under the owner's execution
policy, neither baseline RED nor candidate GREEN was run locally. Existing
fast discovery automatically includes the new stdlib unit file:

```bash
python -m unittest tools.ci.tests.test_provider_python_env -v
python -m unittest discover -s tools/ci/tests -v
```

The tests execute the actual workflow `printf` lines for unset, empty, one-path,
multi-path and space-containing tails, also checking the other exported values
for argument displacement. The authenticated old fragments demonstrate actual
tail erasure. Xiaoge tests AST-extract and execute the actual environment
statements with controlled inputs, avoiding all project peer imports. They
check source retention, both live Python wrappers and provider-only native
children. Receipt tests import real stdlib modules and exercise identity/import
failure detection without third-party dependencies.

For a separate Actions baseline RED, retain the candidate tests/helper and
point only the workflow regression at the original production blob:

```bash
git show 2765b4ab1c15a05c72d23c4ca32c033011c14543:.github/workflows/rust.yml > "$RUNNER_TEMP/provider-python-2765-rust.yml"
PROVIDER_PYTHON_WORKFLOW="$RUNNER_TEMP/provider-python-2765-rust.yml" \
  python -m unittest tools.ci.tests.test_provider_python_env.WorkflowProviderTests.test_actual_printf_preserves_provider_first_and_exact_nonempty_tail -v
```

Expected RED is six assertion failures (the three nonempty tails in each of two
jobs), zero errors and exit 1. Record that exact result before calling it RED;
an unrelated setup error is not evidence. `PROVIDER_PYTHON_SOURCE_ROOT` also
allows the AST peer cases to read a separately preserved production snapshot.

Local preparation passed Python AST parsing for the five affected Python
files, workflow YAML parsing, Bash syntax checks for both affected jobs and the
old fragments, static blob/fragment hash authentication, and `git diff --check`.
Static comparison preserved all 184 disk-guard command lines in their original
order, every other original workflow command/job setting, and the complete
assertion AST sequence in each of the three Xiaoge checks. These are syntax and
diff results, not functional test results.

In **both existing architectures of both affected jobs**, after their pinned
closure and bindings are installed/built, the `before` phase captures real
executable/version/prefix/base-prefix, loaded libpython files and raw mappings,
module import results/paths/versions, search path, architecture and inherited
library path. It records the old fragment's child return code and differences.
The next Actions step, after the actual candidate `GITHUB_ENV` is applied, runs
`after`. Candidate identity, libpython files and imported modules must match
the before receipt; all selected imports must succeed and the provider/tail
composition must match exactly. Failure receipts are written before rejection.

Artifacts retain private, job-owned receipts through the existing upload and
cleanup sequence:

- Webcam: `CI_JOB_TEMP/webcam/python-provider/{before,legacy,candidate}/result.json`.
  The existing recursive `webcam/**/result.json`, invocation and log globs cover
  these files.
- Xiaoge: `CI_JOB_TEMP/xiaoge-python-provider/`, explicitly added to its existing
  artifact. This does not precreate the source comparator's strict output root.

The legacy receipt has no forced RED/PASS condition: it reports any observed
identity/import/process difference or `not-reproduced`. The ARM interpreter's
installed `/opt` path/RPATH may retain its correct prefix even with the old
fragment. A timeout or loader failure without a completed probe is recorded as
such, not presented as proof of a particular libpython/prefix mechanism.

All jobs remain on `bin-actions`, with the original ARM checks in QEMU/PRoot
and no hosted fallback. This is emulated ARM execution, not physical ARM CPU
or vehicle acceptance. Pending acceptance is parent review, fast regressions,
the actual x64 and ARM webcam/Xiaoge receipts and complete unchanged source
comparisons, followed by the ordinary required gates for the new exact head.
