# Independent Sentry receiver wires

`original-http.json` is unchanged output of the original
`rust/tools/check_sdk_receiver.py`, run on 2026-10-19 in the #296 checkout using
Python 3.12 and the existing private bindings. All three cases actually POSTed
identity/gzip/Brotli envelopes to the original threaded loopback receiver.
No Rust candidate, build, install, or external network service participated.

Source SHA256:

- check_sdk_receiver.py: 38cb90685b559628392e0de2578eb295502c7e889c93264a3c079ba15ec81dda
- check_crash_sdk_transport.py: 8bf1281bbc182ecaba9a29b9e49aa3d250ec8ee1f43eca5861596cee475ed806

The compressed gzip timestamp and original ephemeral Host port remain untouched.
Tests replay the exact wire bytes with a newly bound Host port and additionally
exercise owned accepted-socket shutdown and total request deadlines. Those owner
controls are native robustness checks, not claims about original teardown safety.

`original-response-error.json` is a separate unchanged-original capture: all
three complete requests remained queued after the peer sent a TCP reset before
the response. Original stderr confirms acknowledgement writes raised
ConnectionResetError. Native tests must preserve delivery before acknowledgement;
failed parsing or decompression must still produce an error rather than an event.

`original-sdk-fields.json` retains only the **original external Python SDK**
side of the fourteen real transport comparisons in CI run39153248437, artifact
12146317130, telemetry job117752240551 (PASS). Original `comparisons.json` hash
and source identities are embedded. The checked merge
`aa0f9639f882fb62de5a45160eb0d21b5c867534` has tree
`0416b71d5b6bd76891ca58adeb144180cb3f2c6c`, identical to source HEAD
`2106ad4791d488a6361f93b85a8b43eb3d2de1fe`; GitHub's commit API verified both
parents and the tree. The pinned external SDK was sentry-sdk2.55.0.

Extraction selects `expected`, never `actual`, and uses Python's lossless JSON
integers and escaped Unicode to preserve101digit integers and lone surrogates.
It excludes random SDK IDs/timestamps from the selected project-field contract.
The dirty metadata Git commit must be bound to current independent source calls
at comparison time; a historical commit is not an expected runtime answer.
SDK implementation internals, retries and native exception representation remain
outside the original seven-field comparison, as documented by the original tool.
