# RadarCAN comparison startup ordering (#210)

The ARM comparison on `d1cb45166d80479f0376f084bbd2f2db7acfe5fc`
failed before delivering CAN input: the producer awaited subscribers while both
daemons awaited CarParams. The parent released CarParams only after the producer
reported connected subscribers. Original C++ publisher initialization resets
reader registrations, so creating a publisher after a daemon registered its
reader caused this cycle.

[Failed ARM job](https://github.com/bin9208/openpilot-rust/actions/runs/37169141943/job/111338517909)
retains the producer timeout and source CarParams-wait traceback. This was a
comparison harness defect; the original and native radar policies are unchanged.

The producer now reports when all input publishers exist. The parent waits for
that stage before launching either daemon, then retains the existing subscriber,
CarParams, constructor and input-release barriers. Publisher startup failures and
timeouts terminate the comparison through its existing cleanup path.

## Local reproduction and validation

An owned wrapper delayed the actual producer process by one second. Against the
old harness, it reproduced the same subscriber timeout; against the corrected
harness, the same source and frozen native daemon completed 65 input steps and
63 exactly matching publications. No timing threshold or output comparison was
relaxed.

The complete normal corpus passed 23 cases, 2,095 input steps and 960 matched
publications. Joined-input cases passed five cases, 164 steps and 61 matched
publications. These use real original C++ msgq, original Python radar main and
the native daemon SHA-256
`19aac96dd834c738b7acd0248667d11b536cb275fb8100ff8209fcf2080e28ad`.
Ruff and diff checks passed.

Raw commands, source/native messages, delayed RED/GREEN logs and teardown
receipts are retained under `.omo/evidence/radar-ipc-210/` in the issue worktree.
Corrected hosted ARM validation remains required before closing #210. Host IPC
evidence does not establish device behavior or full-runtime completion.
