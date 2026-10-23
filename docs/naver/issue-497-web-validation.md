# Focused web acceptance registration: #252, #254, #270

The acceptance audit at source `82e1284d8602120f1e5aa834ea4cea0d2a41fb42`
found Rust fixes and source controls without a registered full web execution
path. The focused `web` lane previously ran baseline RED, library tests and
Clippy. This change registers the existing controls in
`.github/workflows/backlog-fixes-497.yml`; it changes no production Rust or probe
implementation. It leaves `.github/workflows/rust.yml`, its frozen 184 disk
guard calls, other lanes and required-check routing untouched.

Tracking and runtime behavior:
[report #252](issue-252-audit-fix.md),
[live navigation #254](issue-254-audit-fix.md),
[terminal cleanup #270](issue-270-audit-fix.md).
The existing deliberately narrow #252/#254 oracle exceptions remain exactly
as written. No comparison waiver is added.

## Execution policy and prerequisites

Every Actions job stays on the registered `bin-actions` local runner, with the
existing source-trust guard and no GitHub-hosted fallback. This focused lane is
Linux x64. Any separately required ARM execution must use the existing neutral
QEMU/PRoot foundation; this patch does not duplicate that foundation or replace
ARM cases with x64 cases. Emulation is not physical ARM CPU acceptance.

Functional work runs only in Actions. Local work for this patch is restricted
to AST/YAML/Bash syntax and diff review. No local project imports, unit tests,
bindings, builds, PTYs, network fixtures or dependency installs are authorized.
No run was dispatched/cancelled and no GitHub mutation, commit or push was
performed by this worker. New-head Actions results remain pending.

The existing operation-specific `card_ci_space.py --growth-mib 8192` admission
precedes web dependency/binding preparation and executable builds. No retired
fixed free-space rule is introduced. The job timeout remains 40 minutes, and
every existing probe timeout/deadline remains unchanged. Missing namespace
privileges or dependencies must fail visibly, not skip a proof or fall back to
another runner.

## Transitive dependency and build plan

1. Reuse the existing native compiler/Cap'n Proto/FFmpeg/ZMQ prerequisites and
   Rust 1.94.0 with Clippy. Install `iproute2`, `util-linux`, `openssl` through
   the existing `apt-native` wrapper for the actual main probe's namespace,
   loopback and HTTPS receiver. No global architecture/source reset is used.
2. Reuse the existing setup-python 3.12 and setup-node 24 Actions. Python pins
   reuse the repository's existing CI source closures: numpy 2.5.3, pycapnp
   2.1.0, Cython 3.2.4, setuptools 82.0.1, zstandard 0.25.0, pyzmq 27.2.0,
   requests 2.34.2, aiohttp 3.13.3, anyio 4.12.1, psutil 7.2.2, pycryptodome
   3.23.0, setproctitle 1.3.7, pyserial 3.5, tqdm 4.70.1 and PyYAML 6.0.3.
   Msgpack 1.2.1 comes from the existing `uv.lock`. The browser assertion uses
   the real JS normalizer and Node's built-in assertion module; no browser
   automation package or substitute normalizer is introduced.
3. Run `build_params_python.py` and `build_visionipc_python.py` into owned
   `CI_JOB_TEMP/web-bindings/{params,ipc}`. The latter also builds unchanged
   C++ msgq. The Params builder retains its hash-verified json11 wheel from
   `uv.lock`, native compiler commands and original-source hashes. Read the
   actual extension path from its `provenance.json`; export it as
   `ORIGINAL_PARAMS_BINDING`. Put the original IPC output, checkout, opendbc and
   `rust/tools` on `PYTHONPATH`. Do not alter inherited library paths.
4. Import the actual Params extension, msgq, VisionIPC and pinned Python
   modules in Actions and retain interpreter/module paths and package versions.
5. Preserve the existing `--bins` build of `openpilot-carrot-server` and
   `openpilot-process-supervision`, including installed sibling supervisor
   entry points. Build exactly the needed examples: `dashcam_report`,
   `carrot_live`, `carrot_terminal`, `carrot_terminal_command`, `carrot_main_owner`.
   Compile the existing narrowly scoped TIOCGPTPEER/ENOTTY seccomp fixture and
   record executable SHA-256 identities. No production build flags are waived.
6. Keep the existing baseline web RED path and all library tests/strict
   all-target Clippy. Library tests run serially with `--nocapture`, preserving
   fixture framing/setup diagnostics and cleanup assertions in an artifact log.

## Registered complete probes

All commands are visible in the focused workflow with their real required
arguments, default full case lists, fresh output roots and `set -euo pipefail`.
Every probe's stdout/stderr is retained with `tee`; a failed probe fails its
step. No `--only-zstd`, `--reuse-codec`, `--start-case`, replacement provider or
minimum-case shortcut is used to claim acceptance.

| Scope | Existing probe registration |
| --- | --- |
| #252 | `carrot_server_dashcam_report_source_controls.py`, `..._state_controls.py`, `..._open_stream.py`, then `carrot_server_dashcam_report.py` consuming all three capture roots and actual `dashcam_report` |
| #254 | `carrot_server_live.py` with fresh full compact codecs and original/native msgq/HTTP/WebSocket checks; this invokes `carrot_server_navigation_ui.mjs` on the actual native HTTP payload; then `carrot_server_live_app.py` for composed routing and accepted-WebSocket shutdown |
| #270 reset/EOF | `carrot_server_terminal_edges.py` in both modes, once original and once native per mode; native uses the real process-child launcher |
| #270 PTY | `carrot_server_terminal_pty.py` for original, native, and native peer-open fallback |
| #270 CLI/HTTP | Full `carrot_server_terminal_cli.py` and `..._cli_errors.py`; `..._http.py` standalone and with `--application`, preserving original/native comparisons |
| #270 lifecycle | `..._limits.py` in history/shutdown/missing/params modes; `..._capture.py` immediate/disconnect/timeout/force, retaining the original 45-second timeout and owned-child reaping assertions |
| Main entry | `carrot_server_main_control.py` in term/supervisor/force modes, using actual `openpilot-carrot-server`, `carrot_main_owner` and installed sibling `openpilot-process-child` |

The existing CLI/HTTP fixtures use their explicitly owned tmux/camera/encoder
adapters and **real** C++ VisionIPC/loopback readiness as originally written.
That is not validation of physical cameras, encoders or vehicle services.
Force capture uses the existing helper's native-only owner operation; the
immediate, disconnect and timeout controls compare both original and native.

## Bounded filesystem and network views

The PTY example and original peer require their output roots to descend from
cwd. They also read relative `openpilot/...` source paths. Running them directly
from the checkout with `/tmp` outputs would violate that harness contract.
Instead their cwd is `CI_JOB_TEMP/web-source`, with a symlink to the exact
checkout's `openpilot` directory and outputs beneath `evidence/`. Imported source
modules still come from the checkout; no handler implementation is copied or
changed. The short owned path also avoids long Unix-domain logging addresses.

The normal main probe explicitly sets `/data/openpilot`, brings loopback up and
binds its held TLS heartbeat receiver to port 443. Its workflow wrapper runs in
`sudo -n unshare --mount --net`, first makes mounts recursively private, and
creates a chroot below the owned temp root. It binds required host tools/source
read-only and exposes the exact checkout at `/data/openpilot`. A committed web
tree archive with a recorded Git tree ID is a separate writable submount so
normal startup can produce compressed sidecars without modifying the checkout.
Only the namespace's resolver view maps the original heartbeat hostname to its
loopback receiver. The self-signed certificate/key are test-only files under
the owned temp root; no host certificate trust policy or production TLS code
changes. There is no external network route or host `/data` provisioning.
The archive is created as the checkout owner before sudo. Git subprocesses in
the root namespace receive a process-local `safe.directory=/data/openpilot`
for that read-only bind only; no global Git config or wildcard trust is written.

Mount/net namespace, chroot and noninteractive sudo support are runner
prerequisites and still need proof in Actions for this new wiring. Existing
native probes have not been edited to pretend that missing privileges or a
failed normal startup passed. The main helper retains its observed-UID and
`USER=comma` scope statement; this is neither a claim about a physical vehicle
login nor a complete device-runtime acceptance.

## Evidence and pending acceptance

The existing always-run upload precedes the existing ownership-checked job-temp
cleanup. Added artifact paths are:

- `CI_JOB_TEMP/web-source/evidence/`: source SHA, native hashes, actual import
  paths, package list, library log, all report/live/terminal captures and logs,
  `live/family/navigation-browser.json`, main stdout and web-tree identity.
- `CI_JOB_TEMP/web-bindings/params/{provenance,commands}.json`: unchanged Params
  build/source/dependency evidence.
- `CI_JOB_TEMP/web-main/{term,supervisor,force}/`: actual main HTTP, WebSocket,
  PTY, owner stop, receiver and cleanup receipts. The receiver private key at
  the parent directory is not uploaded. Namespace root/tool/source views are
  also excluded from artifact paths.

`tools/ci/tests/test_backlog_web_registration.py` is included by existing fast
unit discovery. It reads the actual workflow and argparse ASTs without project
imports, checks required arguments/full modes, real binding/Node/build
prerequisites and upload-before-cleanup routing. This policy guard does not
execute or replace any acceptance probe. Actions commands:

```sh
python -m unittest tools.ci.tests.test_backlog_web_registration -v
python -m unittest discover -s tools/ci/tests -v
```

An Actions-only registration RED can retain the new test and point it at the
original production workflow, without modifying the probes or runtime:

```sh
git show 82e1284d8602120f1e5aa834ea4cea0d2a41fb42:.github/workflows/backlog-fixes-497.yml > "$RUNNER_TEMP/web-registration-82.yml"
BACKLOG_WEB_WORKFLOW="$RUNNER_TEMP/web-registration-82.yml" \
  python -m unittest tools.ci.tests.test_backlog_web_registration -v
```

Expected RED is four missing-registration assertion failures, not an import,
syntax or setup error. It has not been executed locally. This policy RED is
separate from the three issues' actual original-production runtime RED proofs.

Local static validation passed Python AST parsing of the new policy test,
workflow YAML parsing, Bash syntax for every focused/Java step and the nested
namespace body, and AST syntax for embedded Python bodies. `git diff --check`
passed. Static comparison retained the original triggers, matrix, labels,
trust guard, focused timeout/env and diagnostic-close job. `rust.yml` and its
184-guard inventory remain untouched. No policy or runtime test was executed.

## #509 guard-owner coordination

The parent reported the actual `82` workspace failure at job `120120936136`
(22:39:17): Clippy passed, then three model-runtime tests rejected the retired
25 GiB floor. Parent owns the Rust export guard and Gauss owns the Python
capacity-policy follow-up. This registration does not edit either guard set.

A read-only flat-module import inventory of the new web/build probes reached
37 source files. Retired reserve checks remain in the standalone `main()` paths
of `rust/tools/carrot_server_dashcam_catalog.py` (58–59),
`carrot_server_dashcam_media.py` (106–108), and
`carrot_server_dashcam_upload.py` (225). These modules are transitive prerequisites
here for `source_modules`, `read_response` and `save`; these standalone mains
are not invoked by this registration. They are reported to the parent for
Gauss's global registered/transitive policy inventory, not silently edited or
waived. The directly invoked terminal CLI admission is already operation-only
(4608 MiB growth); the workflow retains its operation guard before dependency
and build preparation. Exact-source functional acceptance still belongs to
the next parent-reviewed Actions head, bundled with #509 as appropriate.

Record the next candidate's exact SHA and Actions job/artifact links after parent
review and its necessary source push. Require baseline RED for the documented
runtime mechanism, candidate library/Clippy GREEN, full source comparisons,
browser and terminal/main lifecycle results, and ordinary required gates before
closing the three issues. Registration and syntax/diff review alone establish
none of those runtime outcomes. Device/drive and physical ARM acceptance remain
separate and are not fabricated by these synthetic host probes.
