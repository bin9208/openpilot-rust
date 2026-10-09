//! libusb-1.0 declarations from the retained LGPL header in native/vendor/libusb.h.

use std::ffi::{c_int, c_void};

pub(super) type Pointer = *mut c_void;
pub(super) type Unary = unsafe extern "C" fn(Pointer);
pub(super) type Interface = unsafe extern "C" fn(Pointer, c_int) -> c_int;

#[repr(C)]
#[derive(Default)]
pub(super) struct Descriptor {
    length: u8,
    kind: u8,
    usb: u16,
    class: u8,
    subclass: u8,
    protocol: u8,
    packet_size: u8,
    pub vendor: u16,
    pub product: u16,
    device: u16,
    manufacturer: u8,
    pub product_string: u8,
    serial: u8,
    configurations: u8,
}

#[repr(C)]
pub(super) struct RawTransfer {
    pub handle: Pointer,
    pub flags: u8,
    pub endpoint: u8,
    pub kind: u8,
    pub timeout: u32,
    pub status: c_int,
    pub length: c_int,
    pub actual: c_int,
    pub callback: Option<unsafe extern "C" fn(*mut Self)>,
    pub user_data: Pointer,
    pub buffer: *mut u8,
    pub iso_packets: c_int,
}

const _: () = assert!(std::mem::size_of::<Descriptor>() == 18);
const _: () = assert!(std::mem::offset_of!(Descriptor, vendor) == 8);
const _: () = assert!(std::mem::offset_of!(Descriptor, product_string) == 15);
// Both supported runtime targets use the same LP64 libusb ABI; no iso packets are allocated.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<RawTransfer>() == 64);
    assert!(std::mem::align_of::<RawTransfer>() == 8);
    assert!(std::mem::offset_of!(RawTransfer, callback) == 32);
    assert!(std::mem::offset_of!(RawTransfer, user_data) == 40);
    assert!(std::mem::offset_of!(RawTransfer, iso_packets) == 56);
};
