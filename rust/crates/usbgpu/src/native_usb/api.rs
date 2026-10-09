use super::abi::{Descriptor, Interface, Pointer, RawTransfer, Unary};
use crate::Error;
use libloading::Library;
use std::{
    ffi::{c_char, c_int, CStr},
    rc::Rc,
};

pub(super) struct Api {
    pub init: unsafe extern "C" fn(*mut Pointer) -> c_int,
    pub exit: Unary,
    pub list: unsafe extern "C" fn(Pointer, *mut *mut Pointer) -> isize,
    pub free_list: unsafe extern "C" fn(*mut Pointer, c_int),
    pub descriptor: unsafe extern "C" fn(Pointer, *mut Descriptor) -> c_int,
    pub bus: unsafe extern "C" fn(Pointer) -> u8,
    pub address: unsafe extern "C" fn(Pointer) -> u8,
    pub open: unsafe extern "C" fn(Pointer, *mut Pointer) -> c_int,
    pub close: Unary,
    pub device: unsafe extern "C" fn(Pointer) -> Pointer,
    pub string: unsafe extern "C" fn(Pointer, u8, *mut u8, c_int) -> c_int,
    pub active: Interface,
    pub detach: Interface,
    pub reset: unsafe extern "C" fn(Pointer) -> c_int,
    pub configure: Interface,
    pub claim: Interface,
    pub release: Interface,
    pub alternate: unsafe extern "C" fn(Pointer, c_int, c_int) -> c_int,
    pub clear: unsafe extern "C" fn(Pointer, u8) -> c_int,
    pub streams: unsafe extern "C" fn(Pointer, u32, *mut u8, c_int) -> c_int,
    pub free_streams: unsafe extern "C" fn(Pointer, *mut u8, c_int) -> c_int,
    pub control: unsafe extern "C" fn(Pointer, u8, u8, u16, u16, *mut u8, u16, u32) -> c_int,
    pub bulk: unsafe extern "C" fn(Pointer, u8, *mut u8, c_int, *mut c_int, u32) -> c_int,
    pub strerror: unsafe extern "C" fn(c_int) -> *const c_char,
    pub alloc: unsafe extern "C" fn(c_int) -> *mut RawTransfer,
    pub free: unsafe extern "C" fn(*mut RawTransfer),
    pub submit: unsafe extern "C" fn(*mut RawTransfer) -> c_int,
    pub cancel: unsafe extern "C" fn(*mut RawTransfer) -> c_int,
    pub stream_id: unsafe extern "C" fn(*mut RawTransfer, u32),
    pub events: unsafe extern "C" fn(Pointer, *mut libc::timeval, *mut c_int) -> c_int,
    _library: Library,
}

impl Api {
    pub fn system() -> Result<Rc<Self>, Error> {
        // SAFETY: this fixed trusted native dependency implements libusb-1.0; the owner retains its library.
        let library = unsafe { Library::new("libusb-1.0.so.0")? };
        macro_rules! load {
            ($name:literal) => {{
                // SAFETY: each destination function type matches the retained libusb.h declaration.
                unsafe { *library.get(concat!("libusb_", $name, "\0").as_bytes())? }
            }};
        }
        Ok(Rc::new(Self {
            init: load!("init"),
            exit: load!("exit"),
            list: load!("get_device_list"),
            free_list: load!("free_device_list"),
            descriptor: load!("get_device_descriptor"),
            bus: load!("get_bus_number"),
            address: load!("get_device_address"),
            open: load!("open"),
            close: load!("close"),
            device: load!("get_device"),
            string: load!("get_string_descriptor_ascii"),
            active: load!("kernel_driver_active"),
            detach: load!("detach_kernel_driver"),
            reset: load!("reset_device"),
            configure: load!("set_configuration"),
            claim: load!("claim_interface"),
            release: load!("release_interface"),
            alternate: load!("set_interface_alt_setting"),
            clear: load!("clear_halt"),
            streams: load!("alloc_streams"),
            free_streams: load!("free_streams"),
            control: load!("control_transfer"),
            bulk: load!("bulk_transfer"),
            strerror: load!("strerror"),
            alloc: load!("alloc_transfer"),
            free: load!("free_transfer"),
            submit: load!("submit_transfer"),
            cancel: load!("cancel_transfer"),
            stream_id: load!("transfer_set_stream_id"),
            events: load!("handle_events_timeout_completed"),
            _library: library,
        }))
    }

    pub fn text(&self, code: i32) -> String {
        // SAFETY: strerror returns libusb's immutable, NUL-terminated string valid while this library lives.
        let pointer = unsafe { (self.strerror)(code) };
        if pointer.is_null() {
            return "libusb returned null error description".into();
        }
        // SAFETY: the preceding libusb return has the documented static C string lifetime.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }

    pub fn checked(&self, code: i32, operation: &'static str) -> Result<i32, Error> {
        if code < 0 {
            Err(Error::UsbApi {
                operation,
                code,
                message: self.text(code),
            })
        } else {
            Ok(code)
        }
    }
}
