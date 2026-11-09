# device-inputs

`rust/tools/device_build.sh` unpacks these on the device, so a branch that was received with Git builds with no network.

- `sdk`: Rust 1.94 for aarch64, vendored crates and the FFmpeg, OpenCV, raylib, capnp, libyuv, Eigen and ONNX Runtime SDKs. Unpacked outside the checkout.
- `assets`: fixed runtime files that are not compiled here: converted models, shared libraries and Panda firmware. Unpacked into the checkout.

The bundles are split because GitHub rejects files over 100 MB. Regenerate them when `rust/Cargo.lock`, a model or a library changes; `MANIFEST` selects when a device unpacks them again.
