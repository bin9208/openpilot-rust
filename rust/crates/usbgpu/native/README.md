# Native USB boundary

`usb.cc` and `transfers.cc` are project-owned CXX adapters. USB/PCIe protocol
policy lives in Rust. The adapters dynamically load the system
`libusb-1.0.so.0`; loading and device access occur only when `Usb::open` is called.

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
submission/event errors. No native call retains Rust storage after returning.
`USBGPU_SANITIZE=1` instruments the CXX boundary with ASan and UBSan for host
fixture validation. The native fixture exports a local libusb ABI and never
opens actual USB hardware.
