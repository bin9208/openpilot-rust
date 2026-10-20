# Bluetooth config controller comparison

The native controller translates the complete fixed schedule in
`rust/tools/check_bluetooth_config.py`. It independently applies the pinned
`model.validate_config` policy to produce expected responses. It checks every
candidate JSONL response with Python JSON equality, and separately checks device
and mapping key order. The 30-second fixture deadline, input and output files,
source and executable identity, cancellation, and no-clobber receipt are covered.
The original checker used `mkdir(exist_ok=True)` for its output. The native
controller requires a fresh output directory so an existing report or foreign
artifact cannot be mistaken for this comparison's result.

The mandatory cloud test runs the unchanged Python checker with the same fresh
`config_fixture`. It compares the full dynamically generated input JSONL bytes,
all original response rows and ordered maps, and the four original summary
fields. `BLUETOOTH_CONFIG_EVIDENCE` receives the original input, original
responses and report, plus the native report. These are actual original
observations; no Rust candidate result supplies the expected values.

Local preparation is limited to source inspection, syntax/formatting and diff
checks. No local build or functional execution is claimed. The Python checker
must stay present until its cloud comparison has been reviewed and a separate
retirement gate has been established.

The CLI holds its report as a temporary file until signal handlers are removed
and the final cancellation flag is checked. The real lifecycle tests send
SIGINT only to the checker leader and exercise its 30-second candidate timeout;
both require cleanup of the acknowledged child and descendant and no receipt.

`original-observation-39507901490/` contains only the unchanged Python 3.12
checker's input, original responses, and receipt from Actions run 39507901490,
job 118921359333, checkout `9845db2db390bc904401a3794b723b510e3bef84`
(workflow head `6b1f6bb015cc9497237beb39fb56c5e7a37b274c`).
The archive identity and each copied member's exact SHA-256 are in
`provenance.json`; the failed overall Bluetooth job does not weaken the passed
config case. No native result was used to derive these expectations.

The historical checker may now be either its exact pinned regular source or
absent. The source binding holds the checker parent and rechecks the same
presence choice through stdout and receipt publication. The two other source
pins remain mandatory. A separate cloud reference must compare a fresh native
present lane and a copied-root absent lane using the same fixture and checker
ELFs, exact full input and original response bytes, raw candidate output,
receipt fields, CLI streams, and a successful two-exec trace. The Python file
remains until that gate passes in Actions. The reference test requires
`BLUETOOTH_CONFIG_PRESENT_OUTPUT`, `BLUETOOTH_CONFIG_ABSENT_OUTPUT`,
`BLUETOOTH_CONFIG_ABSENT_ROOT`, `BLUETOOTH_CONFIG_PRESENT_STDOUT`,
`BLUETOOTH_CONFIG_PRESENT_STDERR`, `BLUETOOTH_CONFIG_ABSENT_STDOUT`,
`BLUETOOTH_CONFIG_ABSENT_STDERR`, `BLUETOOTH_CONFIG_PRESENT_EXEC`,
`BLUETOOTH_CONFIG_ABSENT_EXEC`, `BLUETOOTH_CONFIG_ARTIFACTS_SHA256`,
`BLUETOOTH_CONFIG_CHECKER`, and the existing `BLUETOOTH_CONFIG_FIXTURE`.
