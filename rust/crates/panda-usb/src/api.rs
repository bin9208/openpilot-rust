use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_void},
    path::Path,
    sync::Arc,
};

pub(crate) type Pointer = *mut c_void;
pub(crate) type Unary = unsafe extern "C" fn(Pointer);
pub(crate) type Interface = unsafe extern "C" fn(Pointer, c_int) -> c_int;
pub(crate) type Control =
    unsafe extern "C" fn(Pointer, u8, u8, u16, u16, *mut u8, u16, u32) -> c_int;
pub(crate) type Bulk = unsafe extern "C" fn(Pointer, u8, *mut u8, c_int, *mut c_int, u32) -> c_int;

#[repr(C)]
#[derive(Default)]
pub(crate) struct Descriptor {
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
    product_string: u8,
    pub serial: u8,
    configurations: u8,
}

const _: () = assert!(std::mem::size_of::<Descriptor>() == 18);
const _: () = assert!(std::mem::align_of::<Descriptor>() == 2);
const _: () = assert!(std::mem::offset_of!(Descriptor, vendor) == 8);
const _: () = assert!(std::mem::offset_of!(Descriptor, serial) == 16);

pub struct Api {
    _library: Library,
    pub(crate) initialize: unsafe extern "C" fn(*mut Pointer) -> c_int,
    pub(crate) option: unsafe extern "C" fn(Pointer, c_int, ...) -> c_int,
    pub(crate) exit: Unary,
    pub(crate) list: unsafe extern "C" fn(Pointer, *mut *mut Pointer) -> isize,
    pub(crate) free_list: unsafe extern "C" fn(*mut Pointer, c_int),
    pub(crate) descriptor: unsafe extern "C" fn(Pointer, *mut Descriptor) -> c_int,
    pub(crate) open: unsafe extern "C" fn(Pointer, *mut Pointer) -> c_int,
    pub(crate) close: Unary,
    pub(crate) serial: unsafe extern "C" fn(Pointer, u8, *mut u8, c_int) -> c_int,
    pub(crate) active: Interface,
    pub(crate) detach: Interface,
    pub(crate) configure: Interface,
    pub(crate) claim: Interface,
    pub(crate) release: Interface,
    pub(crate) control: Control,
    pub(crate) bulk: Bulk,
    pub(crate) strerror: unsafe extern "C" fn(c_int) -> *const c_char,
}

impl Api {
    pub fn system() -> Result<Arc<Self>, crate::Error> {
        // SAFETY: this fixed soname is the platform's trusted libusb-1.0 dependency.
        unsafe { Self::load(Path::new("libusb-1.0.so.0")) }
    }

    /// Loads an explicitly selected libusb implementation.
    ///
    /// # Safety
    /// The library must implement the libusb-1.0 ABI and its allocation, buffer,
    /// thread-safety and non-unwinding contracts. Its initialization and teardown
    /// code must be safe to execute in this process.
    pub unsafe fn load(path: &Path) -> Result<Arc<Self>, crate::Error> {
        // SAFETY: the selected external library must provide the libusb-1.0 C ABI.
        // The owner retains it until all contexts, handles and function pointers drop.
        let library = unsafe { Library::new(path)? };
        // SAFETY: every signature below matches libusb.h, including variadic options,
        // ssize_t device counts, descriptor layout and synchronous buffer lifetimes.
        let api = unsafe {
            Self {
                initialize: *library.get(b"libusb_init\0")?,
                option: *library.get(b"libusb_set_option\0")?,
                exit: *library.get(b"libusb_exit\0")?,
                list: *library.get(b"libusb_get_device_list\0")?,
                free_list: *library.get(b"libusb_free_device_list\0")?,
                descriptor: *library.get(b"libusb_get_device_descriptor\0")?,
                open: *library.get(b"libusb_open\0")?,
                close: *library.get(b"libusb_close\0")?,
                serial: *library.get(b"libusb_get_string_descriptor_ascii\0")?,
                active: *library.get(b"libusb_kernel_driver_active\0")?,
                detach: *library.get(b"libusb_detach_kernel_driver\0")?,
                configure: *library.get(b"libusb_set_configuration\0")?,
                claim: *library.get(b"libusb_claim_interface\0")?,
                release: *library.get(b"libusb_release_interface\0")?,
                control: *library.get(b"libusb_control_transfer\0")?,
                bulk: *library.get(b"libusb_bulk_transfer\0")?,
                strerror: *library.get(b"libusb_strerror\0")?,
                _library: library,
            }
        };
        Ok(Arc::new(api))
    }
}
