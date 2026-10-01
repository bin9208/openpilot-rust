# controlsd scheduler musl ABI (#169)

The aarch64-musl build of PR #164 at
`432bf55dce233d171c718dbd95ec2fbb7e26f9d0` failed with E0063 in the
`sched_param` initializer. The
[exact job](https://github.com/bin9208/openpilot-rust/actions/runs/36855009115/job/110345804053)
requires four additional fields in libc's musl layout. The GNU-only initializer
did not compile for that target.

Initialize the full integer/time structure to zero before assigning priority
53, following the existing native daemon boundary pattern. The syscall still
selects PID 0, SCHED_FIFO, then core6 affinity in the original order. No
scheduling policy or error behavior changes.

An isolated test crate includes the actual production module. Before the fix,
its aarch64-musl check reproduces E0063. Afterward that check passes. Miri runs
the module's priority/pointer and ordering/error tests for both x86_64 GNU and
aarch64 musl, with strict provenance, symbolic alignment and preemption checks;
both pass. The main controlsd package's two platform tests and strict Clippy
also pass. A preliminary Miri invocation's empty doctest phase failed to find
the cached std metadata; the explicit library-test runs completed successfully.

Commands, fixture and logs are retained in the ignored controls integration
workspace under `scheduler-abi-fixture`, `scheduler-abi-target` and
`scheduler-*.log`. Full fresh CI is required before merging. These checks do not
exercise a physical device scheduler or establish vehicle acceptance.

Docs-Not-Needed: build portability repair preserves existing scheduling.
