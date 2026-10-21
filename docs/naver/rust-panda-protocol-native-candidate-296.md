# Panda protocol checker candidate (#296)

The optional `pandad-protocol-native` feature registers the native
`check-pandad-protocol` controller and its control/reference checks. The
candidate retains the original 16 seed requests, 664 packing cases, 2,380
decoding cases, 130 alert cases, 30,720 alert steps, 1,536 receive-flag cases,
17 invalid lengths and 71 child phases. Expected responses come from the
independent original C++ capture, not native outputs.

The proposed hosted gate borrows the original C++ executable from accepted
run [39625704015](https://github.com/bin9208/openpilot-rust/actions/runs/39625704015),
artifact `12358686227`, ZIP SHA256
`1dffb8ea17d36f68e02b3c59736effbf9667d6225fb13e35eec833f140f321cc`.
It verifies all 14 current C++ build inputs before reuse, freshly captures the
unchanged original controller, then compares all native transcripts and report
fields. Parser, source-first failure, existing-output preservation, cancellation
and non-UTF8 executable-path checks are included. Native paths preserve Python
surrogateescape semantics through the shared raw JSON representation.

The outer Panda source inventory keeps its immutable 151-path historical
fixture. Current native controller modules and executable entry points are
explicit additions, including the previously omitted device ownership module
and device CLI. The proposed checkout/private roster has 158 inputs while only
the already retired outer orchestrator pair is absent. This new roster still
needs hosted validation; it does not alter historical acceptance evidence.

Source, formatting and diff review passed. The new protocol functional gate
has not run. Neither `check_pandad_protocol.py` nor the shared
`pandad_protocol_cases.py` is removed here. The latter remains an active input
to the CAN-I/O checker until that family is converted and verified.
