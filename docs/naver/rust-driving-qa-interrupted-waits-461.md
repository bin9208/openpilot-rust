# Driving QA pipe waits interrupted by msgq notifications (#461)

[39618259994](https://github.com/bin9208/openpilot-rust/actions/runs/39618259994)
passed the six native driving scenarios and all 114 publications against fresh
independent original observations. The subsequent collector scenario failed
before its frames with OS error 4, `Interrupted system call`. This is a host QA
controller failure, not evidence of a vehicle driving-model failure.

The msgq subscriber registers its reader thread and installs a SIGUSR2 handler.
Publishing sends that thread a notification, which can interrupt the controller's
pipe poll or read. The controller previously treated that interruption as fatal.
Commit ecc671d8 retries an interrupted poll or read through the existing loop;
every retry checks cancellation and the original absolute 60-second deadline.
It does not restart the deadline or change model, logging or validity policy.

The first regression used direct pthread FFI and was rejected by the workspace's
`unsafe_code = forbid` rule in
[39625231257](https://github.com/bin9208/openpilot-rust/actions/runs/39625231257)
and the inherited driver gate39625842787. Neither run reached packet comparison.
Commit0e721009 replaces the test FFI with the real safe msgq Publisher/Subscriber
interfaces. An isolated child registers its waiting thread, receives five actual
notifications, then must read its READY line and all five publications. The
namespace remains owned by the parent until child exit. The forbid rule is intact.

The same commit routes the existing live driving, collector and fault integration
checks through their registered native CLI once each. It retains actual argv,
exit status, stdout/stderr and ELF hashes, and compares stdout with the completed
disk report rather than inventing an invocation receipt from a library call.

Hosted [39626904943](https://github.com/bin9208/openpilot-rust/actions/runs/39626904943)
checks source0e721009 with the old wait code plus the new test required to fail,
then restores the fixed source before the full independent original/native,
collector, fault and owned-builder comparisons. The meaningful old-source RED,
fixed-source checks and fresh original collector/fault runs pass. The job then
fails in its original CLI observation wrapper, whose purported ambiguous option
was actually accepted before an existing-output error. The later native live
collector/fault stages did not run. The wrapper must retain actual observations
before validating them; full fixed-source acceptance remains pending.
Keep #461 open. The shared original Python helpers remain required until their
caller and source-absence gates are complete. Device boot, recording, upload and
driving acceptance remain user-owned.
