# Runtime message state and polling

Tracking: [#26](https://github.com/bin9208/openpilot-rust/issues/26), within
the full-runtime #1 and model-integration #6 work. Production daemon selection
is unchanged.

`openpilot-messaging` implements the original FrequencyTracker, SubMaster state,
and native shared-memory PubMaster/SubMaster boundary. The source is
`openpilot/cereal/messaging/__init__.py` and `services.py` at
`a83757c368187e4d91f19cb39bf1441ea5673d4c`. The generated Rust catalog retains all
88 names, queue sizes, frequencies/ranges, log flags and decimation values;
`check_services.py` verifies it directly against the original definitions.

## Preserved contracts

- Poll-all, one poll service and multiple poll services retain their original
  observed-frequency bounds. Non-polled topics are sampled after one collective
  native poll; they do not each introduce a blocking wait. All subscriptions
  conflate and use original per-service queue capacities.
- Seen/updated, receive time/frame, message monotonic time, validity, alive and
  frequency flags retain source behavior, including on-demand defaults, ignore
  lists, low-frequency check exemptions and simulation. Runtime construction
  reads the existing `SIMULATION` environment variable. Simulation does not make
  invalid messages valid or unseen static messages alive.
- Receive time is sampled after decoding, as in the original update loop.
  Duplicate receive times that divide by zero remain an explicit error.
- Cereal payloads are owned and aligned. Segment allocation is bounded by the
  received byte length, then normal repeated reads retain the source's unlimited
  traversal policy. Malformed input is rejected before advancing a frame.
- The CXX batch owns each original msgq queue throughout polling and copies
  returned bytes to Rust. Runtime topic access is explicit; isolated constructors
  retain the `rust-probe-` namespace requirement. This is the existing local
  native-msgq backend; alternate transports are not validated by these tests.

Three source catalog names have no current Event schema field: `navModel`,
`customReservedRawData1`, `customReservedRawData2`. Original and Rust SubMaster
initialization both reject them. The initial all-service assumption was corrected
after reproducing the original error; tests now assert those three errors and
readable defaults for all other 85 entries. Their caller/schema audit remains
open in [#27](https://github.com/bin9208/openpilot-rust/issues/27). No alias or
catalog deletion is introduced by the port.

## Evidence

`check_message_state.py` executes the actual original classes with socket creation
replaced by inert fixtures. Full original cereal messages supply the payloads;
all receive metadata, validity/frequency flags, moving-average state and group
checks are compared after every update. Across 11,205 updates, all discrete states
matched and maximum numeric difference was zero within the predefined 1e-12 bound.
Coverage includes 2,417 passing and 8,788 failing combined checks, healthy/stale
frequency states, long gaps, recent-window recovery and the zero-interval error.

Actual native IPC tests cover collective polling, non-polled timeout, latest
message selection, retained payloads, 1,000 repeated cereal reads, queue capacity,
reader acknowledgements, malformed packets and simulation validity behavior.
The original peer interoperation tests still pass. ASan/UBSan passed the native
transport and VisionIPC tests with the new batch boundary included.

Initial module/poll tests failed before implementation. An oversized segment
header regression failed before allocation was bounded and then passed. Catalog
availability is checked against the full original schema, including its known
errors. Cargo tests, Clippy, format, source comparison and isolated CI policy
checks are required; exact commit/Actions results are recorded in #26.

Reproduce from the repository root after building the messaging examples:

```sh
PYTHONPATH=. python rust/tools/check_services.py
PYTHONPATH=. python rust/tools/check_message_state.py \
  --binary rust/target/debug/examples/state_probe --output /tmp/message-state
```

No device was accessed or modified. These host checks do not establish AGNOS,
vehicle performance, normal complete-runtime startup/logging/upload, or the first
device handoff gate defined in `design.md`.
