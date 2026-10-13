# Combined ARM OpenCV provider for issue 269

The installed candidate now has one external OpenCV 4.13.0 provider for Xiaoge
and webcam: core, imgproc, imgcodecs, videoio and dnn. The source remains pinned
to `fe38fc608f6acb8b68953438a62305d8318f4fcd`, archive SHA256
`6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61`, with
the existing defined-access patch
`7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec`.
The complete source-file manifest is unchanged after the GCC13 ARM build.
The provider uses target FFmpeg 6.1.1 and V4L2, builtin Protobuf and JPEG,
and the source ARM carotene/KleidiCV primitives. Their notices are retained.

The new `build_runtime_opencv.py` reuses the existing webcam/Xiaoge build
configuration and restricts pkg-config to the target FFmpeg prefix. The initial
build failed because OpenCV's GCC include workaround drops paths ending in
`/usr/include`, including the relocated SDK. A checked target-header alias
binds the zlib header without changing OpenCV source. Failed commands/logs remain
in the receipt alongside the successful bounded build.

Fresh ARM CPython/NumPy/OpenCV references match the actual Rust traces in all
22 image/model cases, including three genuine lane and two VASM inputs.
All 28 webcam pixel/error cases and five owned AVI frames, metadata and EOF close
match. A separate JPEG decode matches all 8,316 source bytes. Five-library and
four-consumer ELF dependency/version/strong-symbol closures pass. These are
QEMU observations with the pinned AGNOS loader/providers, without device or
performance acceptance.

`stage_runtime_opencv.py` rejects conflicting files before copying. An owned
conflict control preserved the original bytes. Actual staging and an idempotent
restage copied five libraries, ten relative loader/link aliases, notices and
metadata, and both `openpilot-xiaoge` and `openpilot-webcam`. All hashes match.
The staged seven-root ELF closure passes; staged first-library-path execution
preserves real lane tensor bytes and five webcam frames/EOF behavior.
No C runtime or system libraries are copied by this helper.

The original Xiaoge LFS models are hydrated with their exact hashes and sizes;
their identities and model algorithms are unchanged. Native daemon
hashes are `5beaff2e3cb7b15a15fe7b0a6fa217fbd6adeecda0e45abba6883bba258b3162`
and `d1373df61c3d75149afe2c3604611f2d65220c7f7ef7956db8bc1449744d1d06`.
The complete command/artifact index is
`.omo/evidence/269-opencv-aarch64/receipt.json`; installed provider provenance is
`rust/bin/licenses/opencv/provenance.json`. Complete normal startup/upload and
device comparison remain overall runtime gates owned by the integration task.
