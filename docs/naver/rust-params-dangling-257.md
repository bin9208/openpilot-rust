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
Clippy, formatting and diff checks pass. The affected HTTP comparison remains
pending in the Carrot server integration; it is distinct from these passing
storage tests. These are host storage observations; no device was accessed.
