# PocketFFT source provenance and boundary

The unmodified `native/vendor/pocketfft_hdronly.h` and BSD-3-Clause
`native/vendor/LICENSE.md` come from
[mreineck/pocketfft 33ae5dc94c9cdc7f1c78346504a85de87cadaa12](https://github.com/mreineck/pocketfft/tree/33ae5dc94c9cdc7f1c78346504a85de87cadaa12).
This is the PocketFFT submodule pinned by the reference NumPy 2.5.3 source revision
[dd88c0c19b54ad9ed3533224221285bf0873249a](https://github.com/numpy/numpy/tree/dd88c0c19b54ad9ed3533224221285bf0873249a/numpy/fft).

The small CXX adapter invokes the same non-vectorized complex plan used for a
single 1-D NumPy FFT, passing the forward/inverse factor explicitly. No NumPy or
Python shared library/interpreter is linked or loaded. Project-owned normalized
correlation, smoothing, masks, lag/candidate policy and estimator state stay Rust.
The external numerical kernel is explicitly retained because RustFFT changed
constant-signal candidate decisions under the source's cancellation arithmetic.

The CXX boundary uses an owned opaque plan, pinned unique mutable access and a
bounded mutable slice. The adapter validates size and copies initialized scalar
fields into its own kernel storage; it does not reinterpret or retain Rust memory.
CXX translates native exceptions into Rust errors. There is no custom Send/Sync,
raw pointer reconstruction or manual Rust unsafe block. Native calls are behind
`native-skip-miri`: Miri cannot execute C++ FFT code. Native ownership/length tests
and a focused sanitized kernel run provide separate evidence; no Miri execution
of the foreign kernel is claimed.

Portable builds require a C++17 compiler and the ordinary CXX build support on
host/aarch64; the header is vendored, so no runtime download or Python environment
is needed. Generic cross-build does not establish AGNOS ABI/device behavior.

The per-file whitespace attribute in `native/vendor/.gitattributes` preserves the
upstream header verbatim; project-owned Rust/Python/glue whitespace checks remain
unchanged. `provenance.json` records both upstream revisions and file SHA-256s.

The caller's Rust complex multiplication explicitly follows the pinned NumPy
[FMA operand order](https://github.com/numpy/numpy/blob/dd88c0c19b54ad9ed3533224221285bf0873249a/numpy/_core/src/umath/loops_arithm_fp.dispatch.c.src#L277-L285):
`re = fma(a.re, b.re, -(a.im*b.im))`,
`im = fma(a.re, b.im, a.im*b.re)`.
Both [AVX2/FMA3](https://github.com/numpy/numpy/blob/dd88c0c19b54ad9ed3533224221285bf0873249a/numpy/_core/src/common/simd/avx2/arithmetic.h#L253)
and [aarch64 NEON](https://github.com/numpy/numpy/blob/dd88c0c19b54ad9ed3533224221285bf0873249a/numpy/_core/src/common/simd/neon/arithmetic.h#L288-L292)
use that fused operation. Host fixtures use the pinned NumPy FMA dispatch;
other NumPy dispatches are not claimed equivalent. A generic aarch64 build
still requires separate execution validation.
