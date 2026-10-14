# Native runtime OpenSSL provider (#316)

The complete package review for source `828efb0d2a8c9c9a2fa9e22f76f445832d6cc35d`
found that native webjoystick's required `rust/bin/openssl` was absent. Its
manager binding correctly rejected the incomplete package. The executable is
needed only when its existing TLS certificate/key must be created; original
RSA-4096, 365-day and subject arguments are preserved.

The provider is extracted from the already authenticated AGNOS
`19.8-carrot-bt1` image. Raw image SHA256:
`375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44`.
Compressed source SHA256:
`077d42186def98025ffad9723774c96f45da10c87cc6fedbc58d0cf4fa65ffd4`.
SDK bytes were independently matched to raw-image streams before staging.

| Image path | SHA256 |
| --- | --- |
| `/usr/bin/openssl` | `77268ab5d3ec14a6e479c526d431bb193354b8ee3d9006ca03d40029b142d5e5` |
| `/usr/lib/aarch64-linux-gnu/libssl.so.3` | `4ba43e4182c8fe8813fef851e43b3cc09cd90d418760c0a94b13d1bf5d91cc09` |
| `/usr/lib/aarch64-linux-gnu/libcrypto.so.3` | `60076983c2ea0d6b73cc64fb39cafa0a0f90f220ef000f6effb4aaac47500949` |
| `/usr/share/doc/libssl3t64/copyright` | `6a7da622fe0637a334d2a8fc470852d2ffb77d9a2b2f930f854e32a41ad6ef35` |
| `/usr/share/common-licenses/Apache-2.0` | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |

The executable is packaged as `rust/bin/openssl`, with copyright, full Apache
license and provenance under `rust/bin/licenses/openssl`. libssl/libcrypto and
the loader/libc remain native external dependencies supplied by pinned AGNOS.
The image package identifies OpenSSL `3.0.13-0ubuntu3.15`.

Native ELF closure passed all five objects. QEMU with the actual AGNOS loader
executed `version -a` and the original RSA-4096 certificate generation command,
using the image's OpenSSL configuration. Host OpenSSL independently parsed and
verified the resulting certificate; the generated private test key was removed.
The completed package inventory then reported 63/63 ready catalog entries, and
its sealer included the provider and licensing. Local evidence is under ignored
`.analysis/scratch/2026-10-14-openssl-316/` and the packaged provenance receipt.

This establishes provider identity, dependency resolution and certificate-command
behavior under emulation. It does not establish physical body teleoperation,
on-device TLS startup, complete final-source CI or device acceptance.

Docs-Not-Needed: native provider packaging for an existing optional function;
no setting, user flow or guide behavior changed.
