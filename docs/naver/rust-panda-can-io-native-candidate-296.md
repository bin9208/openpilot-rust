# Panda CAN-I/O checker candidate (#296)

This optional `pandad-can-io-native` family reuses the native protocol packing
and seed generators. It retains 1,085 normal and 1,082 MAXOUT scenarios, 3,857
operations and five child phases with the original 90-second limits. The
healthy-negative MAXOUT exclusion remains explicit. Reply consumption, health
gates, partial-frame/checksum reset, transport ordering, inherited MAXOUT
environment handling and failure artifacts retain the original contracts.

Independent expectations come from the original C++ probe in accepted
[39625704015](https://github.com/bin9208/openpilot-rust/actions/runs/39625704015),
artifact `12358686227`, ZIP SHA256
`1dffb8ea17d36f68e02b3c59736effbf9667d6225fb13e35eec833f140f321cc`.
The proposed hosted gate verifies all 14 current C++ inputs before reusing its
ELF. It compares every original request/response in both modes and includes
11 CLI admissions, source-first failure effects, retained foreign output and
raw native/runner path invocations. No native output supplies expectations.

Protocol and CAN-I/O form one dependency closure because both original
controllers import `pandad_protocol_cases.py`. Their native candidates must
pass before that shared source is removed. Remaining work is the hosted
CAN-I/O comparison, shared absence admission, actual outer caller routing and
one affected family integration gate. Successful unchanged source evidence
will be reused rather than recaptured for each removed file.

The outer source roster now explicitly adds four CAN-I/O controller inputs
to the earlier 158-input candidate roster. Its immutable historical 151-path
fixture is unchanged. Source, formatting and diff review passed; the 162-input
outer integration and CAN-I/O functional gates remain pending. No Python
source is deleted by this commit, and physical CAN/vehicle behavior is not
established by these host tests.
