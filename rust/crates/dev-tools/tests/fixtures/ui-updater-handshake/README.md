# Original embedded UI updater handshake oracle

The original Bash/Python heredoc in `.github/workflows/rust.yml`, under
`Require queued exec error after helper exit`, was extracted unchanged and
executed with Python 3.12 against independent temporary Cargo receipt and strace
fixtures. Extracted source SHA256:
`424fd038565aa4894fe0673c555034d697f826b2e460792b8147590d7edcf10d`.
The full original source and capture script remain in private scratch under
`.analysis/scratch/2026-10-15-ui-handshake/`.

`original.json` retains the source workflow blob, 32 receipt/trace inputs,
original admitted/rejected results, exact strace argv/check/timeout, and whether
trace creation occurred before rejection. `$ROOT` denotes the owned temporary
fixture. Inputs independently exercise malformed JSON and non-object lines;
zero, duplicate, wrong target, false/otherwise truthy tests and empty executable;
null target/profile; failed/timed-out strace; missing trace; delayed EAGAIN
count; same-PID event order; exited status; accepted descriptor; unrelated PID;
missing events; Unicode line separators; and Python JSON non-finite/overflow/
underflow truthiness; lone surrogate fields; and integer digit-limit rejection. Non-finite JSON is admitted by Python's default decoder
and must not silently disappear when migrating the receipt reader.

The checker must retain actual strace fault injection and the existing Rust
`missing_executable_is_visible_start_failure` assertion. Synthetic trace parsing
alone does not validate queued exec error lifecycle behavior. The original
20-second process deadline and inherited output are part of the invocation.

Actual source-matched validation also passed: the unchanged original heredoc
and native CLI ran the same UI library test binary (SHA256
`3cd04b4dd936c50e93791c050b27283a74b34e0d2554519f080dc9c6ef82d50d`)
with the freshly built `openpilot-process-child`. Both real traces show the
single delayed EAGAIN followed by same-worker `waitid` with `CLD_EXITED`, then
successful queued `accept4`. PIDs and addresses naturally differ. Results are
indexed in private scratch `comparison.json`; this is host lifecycle validation,
not device or complete-runtime acceptance.
