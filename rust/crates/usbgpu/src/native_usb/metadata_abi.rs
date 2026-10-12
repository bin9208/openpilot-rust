//! Descriptor layouts from native/vendor/libusb.h; libusb owns their nested arrays.
use std::ffi::c_int;

#[repr(C)]
#[derive(Default)]
pub(super) struct RawEndpoint {
    pub length: u8,
    pub kind: u8,
    pub address: u8,
    pub attributes: u8,
    pub packet_size: u16,
    pub interval: u8,
    pub refresh: u8,
    pub sync_address: u8,
    pub extra: *const u8,
    pub extra_length: c_int,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct RawInterface {
    pub length: u8,
    pub kind: u8,
    pub number: u8,
    pub alternate: u8,
    pub endpoints_count: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub string: u8,
    pub endpoints: *const RawEndpoint,
    pub extra: *const u8,
    pub extra_length: c_int,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct RawInterfaceGroup {
    pub alternates: *const RawInterface,
    pub count: c_int,
}

#[repr(C)]
#[derive(Default)]
pub(super) struct RawConfiguration {
    pub length: u8,
    pub kind: u8,
    pub total_length: u16,
    pub interfaces_count: u8,
    pub value: u8,
    pub string: u8,
    pub attributes: u8,
    pub power: u8,
    pub interfaces: *const RawInterfaceGroup,
    pub extra: *const u8,
    pub extra_length: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<RawEndpoint>() == 32);
    assert!(std::mem::offset_of!(RawEndpoint, extra) == 16);
    assert!(std::mem::size_of::<RawInterface>() == 40);
    assert!(std::mem::offset_of!(RawInterface, endpoints) == 16);
    assert!(std::mem::size_of::<RawInterfaceGroup>() == 16);
    assert!(std::mem::offset_of!(RawInterfaceGroup, count) == 8);
    assert!(std::mem::size_of::<RawConfiguration>() == 40);
    assert!(std::mem::offset_of!(RawConfiguration, interfaces) == 16);
};
