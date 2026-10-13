# Native WebDAV file uploads (#280)

Issue: [#280](https://github.com/bin9208/openpilot-rust/issues/280). Base:
`9200ffe57c385e2fa4a89ff11821c7e9ac710d2f`, branch
`codex/feat-280-qnap-webdav`. This implements the owner's approved file-storage
backend; the private NAS address and credentials are provisioned separately.

## Private device configuration

The device reads `/data/carrot/state/private_upload.json`, or the same relative
path beneath `CARROT_DATA_DIR`. Its JSON object has exactly three string fields:
`url`, `username`, and `password`. `url` is the complete HTTPS WebDAV share root
(the approved share is `/openpilot/`). Embedded URL credentials, queries,
fragments, empty credentials, control characters and a colon in the username
are rejected. TLS certificate verification remains enabled.

Provision the file through the owner's secure file channel, owned by the runtime
user (`comma`) with mode `0600`. The reader opens with `O_NOFOLLOW` and rejects
symlinks, non-regular files, another owner, other permissions and files over
16 KiB. The filename is ignored by Git. It is never a Params key, a web setting,
a settings export, a toggle backup, or an upload metadata field. The runtime
neither serializes nor implements `Debug` for the credentials, marks the Basic
Authorization header sensitive, and returns only redacted configuration,
transport and HTTP-status errors. No password belongs in shell command arguments
or diagnostic captures.

The vendored `ureq-proto` 0.6.4 also suppresses its raw request-header TRACE
dump whenever a header is sensitive. A synthetic logger regression reproduced
the old dump and verifies its suppression across incremental writes, unchanged
Authorization wire bytes, and retained public-header traces. This is a
preventive dependency patch; it does not establish an actual production leak.
Its provenance and scope are recorded in `rust/vendor/ureq-proto/PATCHES.md`.

An absent private file retains the current custom HTTP API selection. A present
but invalid file, an authentication error or a WebDAV transfer failure stops
that primary destination; it does not switch to another upload server. Existing
Carrot Logs and Discord destinations remain independently selected by their
existing configuration.

## Storage and runtime integration

- Normal `carrot_man` tmux uploads and optional toggle snapshots use
  `<branch>/<car name> <device id>/tmux-YYYYMMDD-HHMMSS.log` and
  `toggles-YYYYMMDD-HHMMSS.json`. Branch slashes use the existing `__` convention.
- Dashcam's existing worker writes its selected route log and qcamera files to
  `routes/<car name> <device id>/<segment>/<filename>`. The process protocol
  carries only the private configuration path, and the worker reads the file.
- Recovery's primary tmux destination uses the same native backend.
- `/api/dashcam/upload/test` uses authenticated WebDAV OPTIONS when configured.
  It does not request an HTTP API session or create a remote test file.

Each logical path component is validated before networking and percent-encoded
independently. Empty components, `.`/`..`, slashes, backslashes and controls are
rejected. Files are streamed in at most 64 KiB chunks with Content-Length.
Source growth, shrinkage or modification during transfer fails the upload.
MKCOL accepts 201 or the existing-resource response 405; PUT requires
200/201/204. PUT writes a random temporary filename, and MOVE with a same-origin
Destination publishes the final file only after a successful complete transfer;
MOVE must return 201/204. Redirects are always refused. PUT/MOVE transport or 5xx
failures have at most two full-file attempts. Failed and canceled attempts issue
DELETE for the temporary file, accepting 200/204/404, and report cleanup failure
instead of claiming success. Network loss or forced process termination can
prevent remote cleanup; no final filename is published by an unfinished PUT.

Connection timeout is 10 seconds, send-body budget 180 seconds, response/body
budget 20 seconds each and request budget 240 seconds. Cancellation is checked
between chunks and before MOVE; a blocked network call is bounded by its request
timeout. The existing worker concurrency bound remains 1–6.

Results identify `target: "webdav"`, storage-only operation and relative paths.
They do not expose the private base URL, call the API's `/complete`, create
analysis-result links or imply that a file-storage NAS runs Carrot analytics.
The existing dashcam client displays the returned share text.

## Validation scope

Evidence is recorded locally under `.omo/evidence/issue280/`. The ledger names
each scenario, invocation, executable observable and artifact. Local loopback
fixtures exercise the HTTP WebDAV methods with synthetic credentials; their
test-only client uses HTTP, while the production loader accepts HTTPS only.
Native package and HTTP API regression checks are separate from actual NAS or
device acceptance. This worker used loopback fixtures only and did not connect
to a vehicle, commit, push or open a PR.

The parent executor separately rebuilt the native host client after the module
split and uploaded a tmux capture and a settings snapshot to the owner's real
QNAP share. Its authenticated HTTPS GET verification reported an exact byte
match for both stored files. The implementation worker inspected the two
private JSON receipts through an aggregate-only reader; private addresses,
credentials, device identifiers, remote paths and raw attachments remain outside
this public record. The private receipt names are
`qnap-native-upload-result.json` and `qnap-native-upload-verification.json` in the
parent's current boot-attempt scratch directory. This establishes host-client
WebDAV storage interoperability. It does not establish that the final installed
`carrot_man` button or complete runtime candidate has passed the device
comparison; issue #280 retains that separate gate.

The parent also ran the native `openpilot-dashcam-upload --worker` against the
real share with a uniquely named synthetic route log and a 2 MiB qcamera file.
Both authenticated GET readbacks matched the input bytes; the worker reported
storage-only success without an analysis link. DELETE of only that synthetic
fixture returned 204 and a subsequent PROPFIND returned 404. These private
receipts are `result.json` and `cleanup.json` in the parent's synthetic worker
attempt directory. The parent repeated this scenario using the final worker
after the TRACE patch (SHA256
`9f141f38f6c9ecc24380c4637768bc07f9912def814406a93671d480c49d674a`)
with the same exact-byte readback and cleanup results. The complete installed
runtime comparison remains pending.

Final local checks passed for all five affected native packages, all 156
vendored protocol tests, strict Clippy, formatting and documentation mapping.
The native HTTP health route and worker process returned truthful failures for
invalid/unreachable private configuration, contacted no fallback API and
exposed no synthetic credentials. Legacy HTTP API parity passed 99 transport
cases and all 10 dashcam scenarios. These checks do not substitute for the
combined-source CI or the owner's complete runtime/device acceptance.

Docs-Not-Needed: this private deployment file adds no public setting, Params key
or configuration UI; public user guides would expose internal provisioning.
