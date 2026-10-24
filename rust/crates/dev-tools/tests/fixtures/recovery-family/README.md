# Recovery controller family, source-backed host gate

The unchanged controllers remain the independent behavioral specification. The
new `recovery_family` leaf ports local HTTP wire comparison, terminal
PTY/WebSocket comparison, the interactive support source/native exercise,
native terminal lifecycle/descendant cleanup, pending support-shutdown case,
and all 14 CWP scenarios. Actions/tmux and upload are currently Rust admission
controllers for the **unchanged source-backed corpora**, with the original
Python drivers still supplying independent Git, tmux, multipart and upload expectations.
The actions adapter also independently drives the native HTTP action endpoint
for all 24 actions. Equal-behavior replies compare against fresh original
source receipts; staged native command replies compare against the earlier
native receipt plus a literal staged-verb and forbidden-operation contract.
After that controller runs, Rust creates fresh private Git repositories for
all 24 actions and executes the returned native maintenance commands. A
separate owned tmux session compares actual capture bytes with the original,
and `/usr/bin/script` checks inherited PTY descriptors and stdin. The hosted
gate must seal both `/usr/bin/tmux` and `/usr/bin/script`.
The upload adapter independently invokes the native binary for all 16 modes,
using an owned scripted HTTP recipient. It compares complete native results
and normalized JSON/multipart request records with fresh original receipts;
raw multipart boundary bytes are intentionally excluded, while each binary
part retains exact size and SHA-256. Rust also runs all three
`server_tmux_log` action cases with its own server, shell fixture and HTTP
recipient, checking full replies, requests and captured bytes.
The upload and actions source drivers remain
mandatory until caller and absence proofs. The unchanged terminal lifecycle controller
also remains mandatory for its separately observed source reset race.

The host test target requires absolute paths in these variables:

- `RECOVERY_FAMILY_ROOT`: exact checked-out repository root.
- `RECOVERY_FAMILY_PYTHON`: actual Python interpreter used for unchanged source.
- `RECOVERY_FAMILY_SOURCE_WRAPPER`: unchanged `rust/tools/carrot_recovery_source.py`.
- `RECOVERY_FAMILY_SOURCE_SERVER`: unchanged `openpilot/selfdrive/carrot/recovery/server.py`.
- `RECOVERY_FAMILY_BINARY`: freshly built `openpilot-carrot-recovery` ELF.
- `RECOVERY_FAMILY_CWP_SOURCE`: unchanged `rust/tools/carrot_recovery_cwp_source.py`.
- `RECOVERY_FAMILY_CWP_TEST_BINARY`: freshly built carrot-recovery library test ELF containing ignored `cwp::tests::child_driver`.
- `RECOVERY_FAMILY_SUPPORT_SOURCE`: unchanged `rust/tools/carrot_recovery_support_source.py`.
- `RECOVERY_FAMILY_ACTIONS_SOURCE`, `RECOVERY_FAMILY_ACTIONS_FIXTURES`,
  `RECOVERY_FAMILY_ACTIONS_TOOLS`: unchanged three actions controller/helper sources.
- `RECOVERY_FAMILY_UPLOAD_SOURCE`: unchanged `rust/tools/check_carrot_recovery_upload.py`.
- `RECOVERY_FAMILY_REPO_UPDATE`: unchanged `openpilot/common/repo_update.py`.
- `RECOVERY_FAMILY_ACTIONS_EXAMPLE`, `RECOVERY_FAMILY_UPLOAD_EXAMPLE`,
  `RECOVERY_FAMILY_PROCESS_CHILD`: freshly built native example/child ELFs.
- `RECOVERY_FAMILY_TMUX`: pinned host tmux executable.
- `RECOVERY_FAMILY_HTTP_OUTPUT`, `RECOVERY_FAMILY_TERMINAL_OUTPUT`,
  `RECOVERY_FAMILY_LIFECYCLE_OUTPUT`, `RECOVERY_FAMILY_SHUTDOWN_OUTPUT`,
  `RECOVERY_FAMILY_CWP_OUTPUT`, `RECOVERY_FAMILY_SUPPORT_OUTPUT`,
  `RECOVERY_FAMILY_ACTIONS_OUTPUT`, `RECOVERY_FAMILY_UPLOAD_OUTPUT`: distinct,
  nonexistent evidence directories under the job-owned artifact root.

The source and candidate inputs are held regular files and checked again after
each comparison. The tests retain request bytes/results and process logs on
failure. The original CWP response roster and expected gate values are copied
from its unchanged controller, with each actual result compared to a fresh
original run. The source wrapper's own server source digest is checked for the
HTTP and support pairs. The actions adapter requires all 24 Git/action
comparisons, 12 tool/PTY comparisons, native reboot descendant cleanup and
the original summary. The upload adapter requires all 16 source/native HTTP
wire/result rows and three action rows. Hosted workflow wiring should seal interpreter, source, ELF, and
original-controller hashes before and after the whole family, retain artifacts
even on failure, and continue to run all eight unchanged controllers until
their native replacements have separately passed.

No local functional execution or build has validated this leaf. The parent
owns Cargo feature, module, test-target, binary, and workflow registration.
