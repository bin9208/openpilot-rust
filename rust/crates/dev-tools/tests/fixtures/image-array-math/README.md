# Original NumPy image-array comparison

`original.json` captures 97 independent executions under pinned NumPy 2.5.3 of
exactly the integer subtraction/absolute value, last-axis `any`, sum and maximum
operations used by the polygon checker. The data precedes the Rust implementation;
its first test run failed because the native implementation was still a stub.

The 91 successful and six rejected cases include RGBA/palette/grayscale shapes,
trailing-axis broadcasting in either operand, mismatched dimensions, empty arrays,
scalar/rank-one inputs, the 64-dimension limit, and signed 64-bit overflow. In
particular, NumPy's absolute value of INT64_MIN remains negative; changing it to
a checked/saturating/unsigned difference would change the original report.

The native helper consumes dense row-major i64 samples, validates their shape,
streams broadcast coordinates without allocating a difference image and preserves
the original last-axis grouping. It does not decode images, normalize modes,
replace PIL or retire any Python checker by itself. Decoder conversion/warning
semantics and complete checker integration remain separate pending work.
