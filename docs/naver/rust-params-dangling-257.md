# Params namespace recovery: issue 257

[Issue 257](https://github.com/bin9208/openpilot-rust/issues/257) tracks a native
Params constructor difference discovered while comparing the Carrot server
settings snapshot against the original Cython binding.

When a namespace symlink remains but its target directory has been removed,
the original `common/params.cc` creates a temporary directory and temporary
symlink, then renames the symlink over the namespace path. The Rust constructor
previously created the namespace symlink directly. That operation returned
`EEXIST` for the dangling link, so reopening Params failed. In the actual
settings-snapshot comparison this produced a native HTTP 500 instead of the
original HTTP 200 and recreated namespace.

The native constructor now publishes a temporary symlink with atomic rename
while holding its existing Params lock. RAII removes the temporary link and
directory on failure. Existing namespaces, key validation, value writes and
root synchronization retain their existing behavior.

The focused storage regression first reproduced `AlreadyExists` on the old
constructor. It removes the target, reopens Params, writes through the new
handle, reads through the original handle and checks that no temporary link
is left behind. Evidence is retained under
`.omo/evidence/257-params-dangling/`. All seven Params tests, all-target strict
Clippy, formatting and diff checks pass. The Carrot server integration at
`a642aa0e614407c254738386d3923c45490505c6` with the working settings-snapshot
handler also passes the affected original/native HTTP comparison: both return
200 and recreate the removed target; both return the original 500/errno 13
response for an inaccessible root. These two cases exited successfully in
0.962 seconds; see `.omo/evidence/225-settings-snapshot/constructor-final/`.
They are distinct from the storage tests and do not imply whole-server completion.

Exact-SHA Fast checks passed for the isolated fix
[`041627701`](https://github.com/bin9208/openpilot-rust/actions/runs/37915977544)
and its server-branch inclusion
[`a642aa0e6`](https://github.com/bin9208/openpilot-rust/actions/runs/37916003713).
PR/integration acceptance remains pending. These are host storage observations;
no device was accessed.
