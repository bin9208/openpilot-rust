# JPEG dependency provenance

`native/vendor` contains the libjpeg-turbo 3.1.4.1 release source from
https://github.com/libjpeg-turbo/libjpeg-turbo/releases/download/3.1.4.1/libjpeg-turbo-3.1.4.1.tar.gz

Release archive SHA-256:
`ecae8008e2cc9ade2f2c1bb9d5e6d4fb73e7c433866a056bd82980741571a022`.

The codec, build inputs and license text remain byte-identical to that release.
Integration applies only documentation formatting changes: four seven-equals
Markdown heading underlines gain one equals sign (same rendered heading), one
generated CSS indentation loses leading spaces before a tab, and trailing
blank lines are removed from generated CSS/JavaScript and a license HTML
wrapper. These six files otherwise retain upstream content and notices. This
keeps the inherited repository whitespace/conflict-marker gate active without
excluding the dependency. `vendor-format.patch` records the complete packaging
patch; no C/C++/assembly implementation or runtime behavior is changed.

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

Xiaoge snapshot extension retains the existing `encode(rgb,width,height)` quality75
behavior and error checks. Checked Rust `Layout`, `Color`, `Quality` and `Options`
allow packed RGB quality85 and single-component Gray quality50 through
`encode_with`. The narrow C helper uses the same pinned codec, default colorspace
settings, baseline quality tables and integer slow DCT; no preprocessing or image
policy moves into C/C++. New buffer/quality guards execute in both Rust and CXX.
Pure contracts are available without `native-skip-miri`; actual codec calls require
native byte/ownership/sanitizer checks. Fresh Pillow12.3.0 oracle evidence is retained
under `.omo/evidence/xiaoge-200/opencv-jpeg-*`; its success is not inferred from the
older quality75 comparison.

Cluster JPEG extension adds packed RGBX through the same pinned libjpeg primitive;
alpha is ignored and Source Pillow baseline4:2:0/slow-DCT settings remain unchanged.
`TurboJpeg` loads a selected external `libturbojpeg.so.0` using the already-vendored
TurboJPEG1.2+ public ABI declarations (`tjInitCompress`, `tjCompress2`, `tjFree`,
`tjDestroy`, `tjGetErrorStr`). It owns the library/compressor and every output
allocation; only read-only borrowed RGBA enters the synchronous call and owned
Rust bytes return. Pinning plus `PhantomData<Rc<()>>` prevents transferring a
compressor to another thread. Defaults are RGBA/4:2:0/flags0, the primary
PyTurboJPEG1.8.3 Source wrapper contract. Rust cluster code owns selection,
quality clamping, sticky Auto fallback, profiling and async queues. C/CXX contains
only external primitive calls, ABI/resource ownership and input guards. The
shared TurboJPEG provider is a separate external package input; the runtime
never executes Python. Host positive provider is built from the unchanged
release described above; proof will be indexed under
`.omo/evidence/253-cluster-jpeg-async/`.
