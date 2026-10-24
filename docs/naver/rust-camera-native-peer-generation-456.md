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

The hosted negative control removes only the new Vision join while retaining
the diagnostic and requires message=true/image=false/log=true. No camera runtime,
loaded vehicle or device timing failure/fix is claimed from this QA peer.
Tracked in [456](https://github.com/bin9208/openpilot-rust/issues/456).

The first negative control39570788103 passed rather than reproducing the race:
the parent could poll and join before the child published. It is not RED evidence.
The candidate now waits for an explicit publication marker before collection,
using the same single five-second collection deadline across both waits. Removing
only the Vision join must then expose the missing first image deterministically;
no production transport or timeout is changed.

Full hosted run [39572857596](https://github.com/bin9208/openpilot-rust/actions/runs/39572857596)
passed source `2c50f2a1162b13fcf1e0be1eecf1efbbd556ddd5`, workflow
`a86f319621e4949f06da30ab411bc3a8e9c078df` (Rust1.94). The controlled old join fails after5.01s with exactly
message=true/image=false/log=true. Restored-source native tests, the entire camera
regression including source generation and IPC comparisons, and strict Clippy pass.
Parent checked artifact12333043036 (26,861,456bytes/5,197members, all CRCs), ZIP
SHA256 `a1453db67baf27b55ae3b44fffa330016d0aa90ac6ed67fbbeb65b5918121c27`.
This closes the bounded QA peer defect; it does not establish device camera timing.
