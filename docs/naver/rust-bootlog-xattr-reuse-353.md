# Bootlog snapshot xattr scratch reuse (#353)

Tracking: [#353](https://github.com/bin9208/openpilot-rust/issues/353), within
the independent #296 runtime/tooling track.

The manager invokes bootlog snapshot capture before starting its detached worker.
The former private xattr copier created a 65,536-byte names buffer for every
file/directory and a second buffer for each attribute. One private scratch owner
now passes through the recursive snapshot traversal, reuses these two buffers
and drops them before the worker is launched. Both buffers allocate lazily.

The original contract is manager/helpers.py::save_bootlog calling
shutil.copytree, whose copy2/copystat path copies timestamps, xattrs and mode.
Native traversal/error aggregation and operation ordering remain: file bytes
and closes, timestamp update, list/get/set xattrs, then permissions. The ignored
listxattr NOTSUP/NODATA/INVAL and additional get/set PERM/ACCESS results are
unchanged. Other errors retain the same failed snapshot. Launcher, worker
ownership, Params selection and cleanup policies are unchanged.

The safe ownership regression first failed on the missing scratch API. After
implementation, the same two buffers retain their allocation pointers and
65,536-byte capacities across 16 recursively copied files, two directories and
66 attributes. Actual filesystem tests also preserve empty/short/binary/2,048-byte
values, name/value-tail isolation, missing source errors, modes and nanosecond
mtime, dereferenced links, immutable copies and failed-copy retention.

All nine bootlog package tests passed. Strict all-target Clippy passed in
3.39 seconds; the current snapshot example and process-child build passed in
1.56 seconds. Frozen executables then passed the unchanged original/native
snapshot comparator's ten cases, including nonzero/missing/nonexecutable
children, dangling links, FIFO and character-device handling, cleanup failure,
unreadable directories and detached parent exit.

The comparator explicitly uses this #296 source root on PYTHONPATH. Initial
harness attempts failed before comparison because cached NumPy and the original
binding's libzmq runtime were absent from its environment. Supplying the existing
cached dependencies through PYTHONPATH/LD_LIBRARY_PATH repaired the harness;
no comparator assertions or original sources changed.

Evidence: ignored 2026-10-18-bootlog-xattr directory, with red.log, green.log,
clippy.log, build.log, comparison-native-libs.log and proof.json. The latter
records source/executable hashes and exact test/comparison results.

The crate's unsafe-code policy remains intact: no allocator override or lint
exception was added. Stable buffer ownership is not a measurement of total
allocator calls, device CPU improvement or device memory savings. Device boot,
recording and upload acceptance remain user-owned.

Docs-Not-Needed: internal implementation and validation record without changes
to user settings.
