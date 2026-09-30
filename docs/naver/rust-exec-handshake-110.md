# Bounded native exec handshake (#110)

[Issue #110](https://github.com/bin9208/openpilot-rust/issues/110) was found by
bootlog integration under [#103](https://github.com/bin9208/openpilot-rust/issues/103).
A valid long TMPDIR made the native Unix-socket path exceed SUN_LEN before exec,
while the original Python subprocess started successfully.

CapturedCommand now creates its private control directory under Linux `/tmp`
with an `op-exec-` prefix and explicit mode 0700 at creation. Default tempfile
directory permissions followed the host umask (0775 in the reproduction), so
privacy is now explicit. Child TMPDIR and other inherited environment remain
unchanged. The existing child-owner lifetime removes the control directory.
NativeCommand's separate descriptor path is unchanged.

The focused regression failed with SUN_LEN before the fix and passed after it:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-process-supervision --test long_tmpdir --locked
```

It launches a real inherited child with a greater-than-108-byte TMPDIR, observes
the unchanged value in the child, checks 0700 control-directory permissions and
observes cleanup after the owner drops. Exact-SHA Actions/integration results
remain in the issue/PR record; no device operations are involved.
