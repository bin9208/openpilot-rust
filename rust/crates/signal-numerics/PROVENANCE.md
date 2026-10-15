The optional x86 primitive is the unchanged Intel BSD-3-Clause x86-simd-sort
vendored by NumPy 2.5.3. `vendor/source.json` binds every header and license to the
official source archive SHA256. The build verifies those bytes before compiling
x86-64-v3 and v4 objects. Rust checks the complete source ISA groups, including
OS-supported AVX and extended LAHF, before calling either object. The Rust caller
supplies a borrowed immutable Float32 slice and equally sized initialized indices;
CXX retains neither. Signal selection, validation, model interpretation, and
portable ARM sorting remain Rust owned. This is an external numeric dependency,
not a claim that external SIMD code was converted to Rust or yields vehicle CPU
savings. No x86 numeric C++ object is compiled or linked on ARM.
