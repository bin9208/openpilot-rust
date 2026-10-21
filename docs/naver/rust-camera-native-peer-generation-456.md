# Camera QA peer publisher generation (#456)

Full PR [39567841179/job119114353772](https://github.com/bin9208/openpilot-rust/actions/runs/39567841179/job/119114353772),
head917cd7db, failed the one-shot native backend IPC test at its unchanged
five-second deadline; four other tests passed. No test-private files survived.
The original failure alone does not identify the missing stream.

The helper creates message and Vision subscribers before starting publishers,
then joins only the message publisher generation before releasing one frame.
Publisher initialization invalidates the older Vision reader identity; its next
receive joins at the current tail. The candidate joins that queue while still
empty before releasing the sole publication. This matches the existing message
handshake and preserves queue semantics. Missing-stream diagnostics expose the
message/image/log booleans without changing the deadline or exact fixture values.

A hosted negative control will remove only the new Vision join while retaining
the diagnostic, then require message=true/image=false/log=true. Fixed-source
native tests, complete camera regression and strict Clippy are pending. No camera
runtime, loaded vehicle or device timing failure/fix is claimed from this QA peer.
Tracked in [456](https://github.com/bin9208/openpilot-rust/issues/456).
