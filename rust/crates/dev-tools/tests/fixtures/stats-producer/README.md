# Original StatLog wire observations

`original.json` retains only the four original rows from an actual unchanged
`check_stats_producer.py` execution, with command paths removed. That command
also compared the native examples in all eight original/native scenarios.
Expected values are not native outputs. The pressure source assertion checked
all 1,000 ordered messages and absence of additional messages after 20,000 sends.

Capture dependencies: CPython 3.12.15, pyzmq 27.2.0 / libzmq 4.3.5, pycapnp 2.1.0.
Private raw result SHA256:
`d05b66cbc2ef1416ba5c32913ce1a56da340745a5e2a313a4f40326182142c76`.
Projected original fixture SHA256:
`9984d69f1112d59cbcecd5af38aeb9c4e95a19ca602f0f88a546e750adfa654e`.

The native checker independently implements these wire expectations. The two
Python producer/controller helpers are retired after same-binary before/after
removal proofs and a fresh post-deletion build. Six permanent tests retain real
fork/drop/pressure/type checks, cancellation cleanup, source mutation rejection
and output ownership. CI builds and executes the native checker and examples.
The original `openpilot/system/statsd.py` remains for other comparison consumers;
this capture does not claim its retirement or device validation.
