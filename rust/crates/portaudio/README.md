# PortAudio v19 boundary

This crate owns the dynamic library, stream, and boxed synchronized callback.
It does not synthesize audio policy, select alerts, or perform IPC. PortAudio
is an external native dependency; no audio backend is emulated in production.
Only the deployed LP64 Linux GNU/musl targets are currently built.

`Stream::load(path, render)` initializes PortAudio for mono float32 output
(48 kHz, 4096 frames). `Stream::load_input(path, capture)` uses mono float32
input (16 kHz, 800 frames). Call `open()` for each retry: it terminates and
reinitializes PortAudio, selects the default device, uses its high latency,
and opens the stream with flags zero. Call `start()` once after open succeeds;
`active()` preserves the native status/error. Callers own retry count/delay.
The selected library must implement the trusted PortAudio v19 C ABI.

Output closures receive an initialized exclusive buffer; input closures borrow
initialized samples for the duration of the callback. Both receive PortAudio
status flags and return true to continue or false to abort. Panics are caught
at the C entry and return paAbort. No callback data may escape its borrow.
Closures are Send because PortAudio calls them on its worker. A !Send IPC
publisher must be constructed and retained in callback-thread TLS, never
captured and unsafely marked Send. Dropping such state on callback-observed
stop/error is the caller's responsibility; otherwise TLS cleanup follows
PortAudio worker lifetime/process exit. No input samples are queued here.

Drop stops and closes the stream before terminating the library and releasing
callback storage. `native-skip-miri` gates actual dynamic FFI. With default
features disabled, three tests exercise Rust buffer initialization/borrowing
and panic containment under strict Stacked Borrows and Tree Borrows Miri.
The soundd actual-binary fixture additionally checks output ABI parameters,
callbacks, retries, status and cleanup without any host audio initialization.
Input-specific actual ABI/IPC validation belongs to the micd integration.
