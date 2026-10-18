The immutable `format-original.json` contains 18 inputs evaluated by the
unchanged `SwagLogFileFormatter`, with exact UUID-normalized formatting bytes or
original exception fields. UUID normalization validates RFC4122 version4 and
changes only the generated top-level value. Raw source receipt SHA256 is
`fc7c4e938e187df86f3639a591f84efaeb3b643354c9339ae27d2b10784e0e7d`.
The same capture executes the unchanged rotating-handler class with a controlled
monotonic clock, including its default 60-second, 262144-byte pre-write rollover
and 2500-file retention. No Rust collector, candidate formatter or LogFiles
implementation supplies the expected data.

A separate real original ZMQ/msgq/cereal capture preserves 10 multipart, level,
Unicode and publication-boundary controls, full packet/disk artifacts, SIGINT
exit -2, empty-frame exit1, and original multibyte queue-overflow SIGABRT -6.
Its receipt SHA256 is
`8dfdd4cea0c8796778103d24c0d6a4211e224e0a38497bd26a07cac0f52906cd`.
The successful exact 2Mi-character publication is ASCII; a multibyte string of
the same character length exceeds the C++ msgq queue's three-message capacity
assertion. Those distinct outcomes must not be treated as equivalent admission.
Raw artifacts remain private. Source originals remain retained and mandatory.

The focused native API/runtime test is prepared but has not yet compiled or run.
It requires a freshly built check-managed-entry worker in `MANAGED_ENTRY_WORKER_BINARY`, dispatching
`--managed-collector ENDPOINT ROOT`. It checks all14 successful original formatting
inputs via real IPC, threshold levels, exact disk bytes, fresh UUIDs, unchanged
SIGINT signal exit and rejection of pre-existing/symlink directory state before
IPC creation. Error/rotation captures are retained for implementation-level
regressions; this fixture is not a claim of complete shared collector retirement.
