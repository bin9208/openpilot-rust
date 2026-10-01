# JPEG dependency provenance

`native/vendor` is the unmodified libjpeg-turbo 3.1.4.1 release source from
https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/3.1.4.1/libjpeg-turbo-3.1.4.1.tar.gz

Release archive SHA-256:
`ecae8008e2cc9ade2f2c1bb9d5e6d4fb73e7c433866a056bd82980741571a022`.

The upstream `LICENSE.md`, `README.ijg`, source copyright notices and associated
license files remain in the vendor tree. This software is based in part on the
work of the Independent JPEG Group. Only the static libjpeg API is built;
TurboJPEG, tools, tests and SIMD are disabled. The dependency is an external
codec, not a project-owned runtime port. No Python interpreter is invoked.

The project-owned boundary consists of `native/encode.c`, `native/encode.h`,
`native/bridge.cc`, `native/bridge.h` and `src/bridge.rs`. RGB bytes are borrowed
read-only for the duration of the synchronous call; no input pointer is retained.
The C++ boundary validates exact RGB length and dimensions (1..65500). Its C
helper owns compressor/error state on the heap and contains libjpeg's setjmp /
longjmp entirely within C. It destroys the compressor and frees output on error.
On success C++ owns the returned allocation with RAII, copies into an owned Rust
Vec and frees the original allocation before returning. No borrowed native memory
crosses back into Rust. Exceptions become CXX errors. JPEG quality 75, default
YCbCr 4:2:0 and integer slow DCT match the original Pillow path.

Validation is recorded in `docs/naver/rust-athena-146.md`. The sanitizer driver
instruments the complete codec as well as the C/CXX boundary, checks invalid
sizes, deterministic owned output, and leak detection. Source JPEG comparisons
cover padded NV12 and every UV byte pair at four Y levels; observed bytes match
Pillow 12.3.0's libjpeg-turbo 3.1.4.1 output exactly. This does not claim all codec
versions or all possible images produce identical compressed bytes.
