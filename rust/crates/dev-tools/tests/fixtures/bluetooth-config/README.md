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
