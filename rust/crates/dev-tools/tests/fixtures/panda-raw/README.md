The original Python builders at `30866c90^` were executed under pinned CPython
3.12.15 with controlled real subprocess compiler peers. Source SHA-256 values
are embedded in each capture. Peers write fixed products/generated units and
raw stdout/stderr, without compiling code or touching hardware.

`original.json` records 29 raw-filesystem cases: all seventeen original modes,
five non-UTF-8 checkout roots, and seven first/second command failures. Output,
dependency prefixes, selected USB header paths, and custom compiler executable
paths contain undecodable FF bytes. Command arguments are captured as raw hex;
`BASE` and `ROOT` normalize only the temporary ancestor and source checkout.
Receipts preserve Python filesystem surrogateescape values and keys. Tests
compare parsed Python JSON nodes without requiring dictionary insertion order;
production serialization never reparses filesystem escape text.

`text-original.json` records 42 strict-text cases across all seven command
stages: invalid stdout, invalid stderr, or mixed CR/CRLF/LF plus valid non-ASCII
UTF-8; each exits either 0 or 9. Original subprocess strict UTF-8 decoding fails
before the current command record is appended, retaining compiler-created files
and earlier command records. Valid text undergoes universal newline conversion
before logging/checking status. Non-JSON artifact hashes verify partial products
and copied/generated sources as well as log/manifest presence and contents.
