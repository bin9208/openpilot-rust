# Native QR Brotli bundle

The native QR dependency endpoints use Brotli's encoder and decoder C ABI. They
report actual library availability and paths. They do not install Python wheels
or contact a package server. Existing standalone QR consumers may still use the
system Brotli loader; HTTP QR backup/restore uses the Application's provider.

The final runtime package must stage all three libraries from its **same-target
SDK** here, alongside `manifest.json`. A host fixture is not an AGNOS package.
Keep the SDK's Brotli MIT license and provenance in the package's native notices.

```sh
python3 -P rust/tools/stage_brotli_provider.py \
  SDK/usr/lib/aarch64-linux-gnu rust/native/brotli-bundle \
  aarch64-unknown-linux-gnu 1.1.0
```

The output is a complete package directory, to be placed at `rust/native/brotli`
in the runtime package. The destination must not already exist. The tool follows
SDK version symlinks only within the selected SDK library directory, then writes
regular files named `libbrotlienc.so.1`, `libbrotlidec.so.1` and
`libbrotlicommon.so.1`. The manifest contains ABI contract version 1, the Linux
GNU target, Brotli's packed version integer and SHA-256 for exactly these files.
The declared version is rechecked against both native codec version functions.

Repair validates fixed names, target, regular files, bounded sizes and hashes;
loads and exercises a staged generation; then atomically changes `current` under
the App data parent's `native-deps/brotli` directory. A failed candidate leaves
the existing generation selected and removes its incomplete staging. Status and
ensure retain the original installed/format/configured response contract, while
`provider`, `module_path`, `target`, error details and the install mechanism name
the native implementation. Missing or unusable bundles return a real failure;
QR backup keeps its CQR4 fallback.

Hashes establish integrity against the trusted package manifest, not package
authenticity. The common-library handle is retained until encoder and decoder
handles are released. A process which already loaded the same library SONAME may
reuse that dependency through the platform loader; version checks and a real
roundtrip verify compatibility, but cannot prove that every dependency was
replaced in that process. Fresh-process validation observes all three staged
paths. Package/SDK validation and the final target staging remain delivery gates.

The checked version signatures and compression defaults are from the upstream
[Brotli 1.1 encoder header](https://github.com/google/brotli/blob/v1.1.0/c/include/brotli/encode.h)
and [decoder header](https://github.com/google/brotli/blob/v1.1.0/c/include/brotli/decode.h).
