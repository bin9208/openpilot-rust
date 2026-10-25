# Independent original camera state comparator captures

Unchanged check_camerad_state.py and actual pycapnp 2.1.0 provide all expectations.
Checker/schema hashes, CPython and native Cap'n Proto versions are recorded.
Eight input captures cover frames 1, 2, 99, 100, 101, 109, 120 and 400, including
frame-ID wrap, seeded exposure patterns and exact input hashes. Forty-two
original-main process cases preserve commands, the 180-second timeout,
input/stdio/hash/receipt ordering, sanitizer, wire decoding, equality failures
and partial filesystem artifacts. Twenty real CLI captures include Unicode,
whitespace and underscore integer admission.

Fourteen real custom-schema wire messages exercise unions, null versus allocated
pointers, defaults/groups/lists, signed and unsigned 64-bit limits, Float32 and
Float64 nonfinite values, negative zero, Data and Unicode. Native capnp JSON is
retained as diagnostic material, never as expected source values. It encodes
64-bit integers/nonfinite floats as strings and changes float formatting and
field order. A schema-directed adapter is required. Original code transforms
only camera image Data to lists; other Data causes JSON serialization failure.

The native comparator passes a fresh current-source C++/Rust 1,080-frame run
with PATH disabled. Inputs, output fields and receipts match the original.
Native schema loading uses the external Cap'n Proto compiler and pinned Rust
capnp crate to read its actual schema descriptors. There are no fixed Event
schema bindings or Python wrappers. Four additional original CLI captures
preserve lazy Event lookup: missing/enum Event is accepted without wire rows,
and fails after source output/receipt creation when a wire row needs decoding.
Five process captures preserve list/string wire-membership failure ordering.
Capture/probe drivers are ignored under
.analysis/scratch/2026-10-18-camera-state-check/.
These are host fixture results; device acceptance remains user-owned.
