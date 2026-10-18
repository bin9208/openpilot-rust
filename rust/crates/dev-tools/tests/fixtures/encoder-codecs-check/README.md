# Original encoder codec checker contracts

Captured before native implementation from unchanged check_encoder_codecs.py.
Fourteen main executions preserve source extraction, command ordering, filesystem
side effects, packet comparisons and final/partial artifact bytes. Only external
pkg-config/compiler/codec process and disk availability boundaries are controlled.
This corpus does not establish codec equivalence; all five actual current C++/Rust
codec cases must also pass before retiring the original command.

Cases include success, package launch/quoting errors, disk refusal, each process
failure phase, metadata/count/content/missing packet errors, a missing final
library and an existing output directory. The input and output root substitution
is explicit. Expected data is never loaded by production tools.

The actual source-method gate subsequently passed all five cases (112 packets).
Running both checkers at the same absolute output path produced byte-identical
receipt.json. Different output paths embed different assertion filenames in the
C++ executable; that changes its recorded hash, without changing codec outputs.
The comparison keeps that hash intact and uses identical paths for exact receipts.
