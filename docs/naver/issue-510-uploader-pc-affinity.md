# Uploader PC affinity boundary (#510)

Parent: [#497](https://github.com/bin9208/openpilot-rust/issues/497).
Issue: [#510](https://github.com/bin9208/openpilot-rust/issues/510).
Candidate: PR500 to dev; no dev-md source or device operation.

## Actual failure

[Support job120141047822](https://github.com/bin9208/openpilot-rust/actions/runs/39869685862/job/120141047822)
failed on PR head `804730ab5a57b9c222a27f5e64cf9731803a852d` after
834 typed-Params string comparisons passed. The unchanged real uploader/Params
comparison stopped at `check_params_string_uploader.py:138`, rejecting an
unexpected `errorLogMessage` in missing-id/native with the original collector.
The source case logged only the expected missing-DongleId INFO event. Native
also emitted `failed to set core affinity`, errno22/InvalidInput, before checking
DongleId. No upload/xattr operation had been reached in this case.

Artifact12470339923 ZIP SHA256:
`b21ea13782d650a199e46a5d5b85596574ed6e2ed5185867a776e3274e57aa08`.
It preserves the actual binary IPC error and the source/native collector records.
The exact host allowed-CPU mask was not retained; errno22 alone does not identify
that mask.

The PR workflow checked out merge `51dc30fc59e36100db78533dd0d7ae84c3cac3ae`,
parents `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` and `804730ab...`.
Both candidate and tested merge have tree `a3684db72167962ff462960f4385ef90095bb375`.
The logging context and original Params-binding provenance correctly identify the
actual merge checkout. This is not evidence of a stale logging binary/cache.

## Source contract and correction

The preserved `openpilot/common/realtime.py::set_core_affinity` calls
`sched_setaffinity` only on Linux and `not PC`. Hardware detection uses
`os.path.isfile('/TICI')`, with `PC = not TICI`. Uploader's requested device mask
is cores0..3. The native uploader omitted the PC guard and attempted that mask on
all hosts, including those where this request is invalid. On other PCs it could
silently change placement despite the original no-op.

The scoped native change returns no requested mask on PC and uses the same
`/TICI` file detection already used by native uploader persist paths. Device
requests still contain exactly cores0..3; existing syscall failure diagnostics,
loop/exit behavior, Params warnings, transfer/xattr/backoff and signal handling
remain. There is no new option, error-record filter, CPU/cgroup mutation of the
runner, assertion waiver or deadline increase.

Two native unit regressions require no PC affinity request and exactly the
original device mask across the complete CpuSet range. Existing focused core
`openpilot-uploader --all-targets` and workspace checks execute them. The existing
full support gate still runs `check_params_string_uploader.py` unchanged across
seven inputs, original/native uploaders and both actual log collectors, requiring
exact warning/log/HTTP/upload outcomes and no unexpected errorLogMessage. The
recorded pre-fix CI is the behavioral RED; a compiler/setup failure does not
substitute for it.

## Validation and acceptance

Local validation is syntax/format/diff review only. Actual native unit cases,
all28 collector/uploader combinations, remaining support regressions, required
exact-source PR gates, normal dev merge and separate post-merge checks remain
pending Actions. Device behavior or physical CPU placement was not tested.
Paired Korean/English Rust-port guides describe the host boundary.
