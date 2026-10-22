# Complete ARM/AGNOS runner comparison498

Requested2026-10-23 KST, following the focused comparison496.
Issue: https://github.com/bin9208/openpilot-rust/issues/498 .

Both runners build the same comparison commit, based on dev-md
`dc95138076836ea3a5c1b6c27262be3f3b912f71`. Runtime functionality is unchanged.
Only isolated comparison instrumentation, explicit self-hosted build admission,
and the owner-requested fixed disk-reserve exception differ from that source.

The AGNOS job rebuilds every configured runtime executable and fresh OpenCV,
audits genuine19.8-carrot-bt1 ELF dependency/symbol closure, seals, and verifies
offline ARM/QEMU installation. The generic ARM job retains the existing complete
GNU/musl workspace and native Card/navigation/Panda/camera/encoder/UI steps.

Both use fresh Cargo/target/provider directories, Rust1.94.0 and2 build jobs.
Initial tool installation and downloads are measured separately from builds.
No release publication, main/dev integration, permanent runner migration or
physical-device operation is authorized. Only syntax/format/diff checks run on
the workstation. Results remain pending until both jobs have final evidence.
