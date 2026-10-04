# External OpenCV boundary

Native dependency is OpenCV4.13.0 source revision `fe38fc608f6acb8b68953438a62305d8318f4fcd`, downloaded archive SHA256 `6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61`. Original Python oracle is opencv-python-headless4.13.0.92, revision `b4c5ec4042f097e2a5b386b9d413ec7333d0a184`; its parent is the pinned native revision. Retained comparison shows only two Python test-file edits, without native source edits.

The native build applies `native/opencv-defined-access.patch`, SHA256
`7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec`.
Full-library UBSan exposed unaligned scalar loads in SSE lookup helpers and
negative signed-coordinate left shifts in `CollectPolyEdges`. The patch copies
the same scalar bytes with `memcpy` and multiplies signed 64-bit coordinates by
the same positive fixed-point scale. Validated i32 caller coordinates fit that
wide product. C++11 and all sanitizer checks remain enabled. The builder verifies
the exact original and patched hashes of both affected files, and freezes the
patch, hashes, and modified sources alongside the installed library manifest.
The original wheel oracle remains unchanged. Unpatched library trees and RED
traces are retained separately; patched outputs never replace their libraries.
The checksum-pinned unified diff retains its blank context-line whitespace;
only that patch artifact is exempted from Git whitespace diagnostics.

`rust/tools/build_xiaoge_opencv.py` builds only core/imgproc/dnn, without Python, IPP, OpenCL, LAPACK, GUI, codecs, or CUDA. OpenCV retains its Apache2.0 license; bundled protobuf retains BSD3-Clause, flatbuffers Apache2.0 and zlib1.3.1 its Zlib license. Builder copies complete upstream licenses into its output and freezes library/header/license SHA256 identities in `native-libraries.sha256` and `receipt.json`. Cargo requires `OPENPILOT_OPENCV_ROOT` and verifies that manifest before compiling CXX. No cv2.so or Python runtime is linked.

The CXX declarations accept borrowed slices, validate all extent/product/format contracts twice, clone the DNN input before OpenCV may retain it, and return owned Rust vectors. Exceptions are caught by CXX and returned as typed Rust errors. DnnNet has no Send/Sync implementation and contains an Rc marker. Once-only thread initialization completes before any native operation, avoiding concurrent changes to OpenCV's global thread configuration.

Cropping, stride removal, polygon scaling, normalization, channel ordering, confidence interpretation, scheduling and control remain in Rust callers. The adapters perform external OpenCV calls plus representation/ownership conversion only. JPEG remains a separate external dependency.

Pure contract tests run with `--no-default-features` under Miri. They verify checked dimensions, lengths, shape products and owned/borrowed views; Miri cannot execute the native library. Native exception/ownership checks, exact pixel differentials and real pinned-model comparisons require actual CXX execution with ASAN/UBSAN coverage. Host and aarch64 GNU/QEMU evidence must remain separate; GNU symbol/version and AGNOS sysroot compatibility require explicit checks, and none establishes camera/device acceptance.
