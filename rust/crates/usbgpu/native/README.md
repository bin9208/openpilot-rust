# Native USB boundary

The project-owned CXX adapters were replaced by `src/native_usb/` on 2026-10-09.
Rust owns the library, context, handle, streams and transfer lifetimes and loads
the external `libusb-1.0.so.0` only when `Usb::open` is called. The retained
declaration-only header defines the external ABI and its provenance.

The declaration-only `vendor/libusb.h` comes from Ubuntu's
`libusb-1.0-0-dev` package `2:1.0.27-1` for amd64. Package SHA-256:
`01228d4112c04152ae5976563ac6c00fb1dcd8f620232395da6dc67af975003b`.
The header's upstream copyright and LGPL-2.1-or-later terms are retained in the
header, `vendor/LIBUSB-COPYRIGHT`, and `vendor/LICENSE.LGPL-2.1`.
Only trailing whitespace in the package copyright text is normalized.
This header describes the native ABI; it does not bundle a libusb implementation.

Each native handle owns its library, context, device handle, claimed interface
and allocated streams. A batch retains all transfer records and buffers until
submitted transfers deliver terminal callbacks, including cancellation after
submission/event errors. Public synchronous and batch operations return only
after the native library no longer retains their Rust buffers or callback state.
The current sanitizer control instruments the Rust owner and an owned libusb
ABI fixture with ASan. The obsolete `USBGPU_SANITIZE` CXX build hook is removed.
The fixture never opens actual USB hardware, and its results do not establish
that the production libusb implementation itself was instrumented.
