use super::{
    api::Api,
    metadata_abi::{RawConfiguration, RawEndpoint, RawInterfaceGroup},
    Usb,
};
use crate::Error;
use std::{ffi::c_int, ptr, slice};

#[cfg(test)]
#[path = "configuration_tests.rs"]
mod tests;

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Endpoint {
    pub address: u8,
    pub attributes: u8,
    pub packet_size: u16,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct InterfaceDescriptor {
    pub number: u8,
    pub alternate: u8,
    pub endpoints: Vec<Endpoint>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Configuration {
    pub value: u8,
    pub interfaces: Vec<InterfaceDescriptor>,
}

type Get = unsafe extern "C" fn(super::abi::Pointer, u8, *mut *mut RawConfiguration) -> c_int;
type Free = unsafe extern "C" fn(*mut RawConfiguration);

pub(super) struct Symbols {
    pub get: Get,
    pub free: Free,
}

struct OwnedConfiguration<'a> {
    _api: &'a Api,
    free: Free,
    pointer: *mut RawConfiguration,
}

impl Drop for OwnedConfiguration<'_> {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            // SAFETY: category12; this sole owner frees the returned tree once before its library unloads.
            unsafe { (self.free)(self.pointer) };
        }
    }
}

impl Usb {
    /// Copies a cached SDK configuration descriptor; does not issue a control transfer.
    /// # Errors
    /// Returns missing optional symbols, SDK errors or invalid descriptor tree layouts.
    pub fn configuration(&self, index: u8) -> Result<Configuration, Error> {
        let symbols = self.api.configuration_symbols()?;
        // SAFETY: category3; this live handle retains its device for the synchronous metadata query.
        let device = unsafe { (self.api.device)(self.handle) };
        if device.is_null() {
            return Err(Error::Contract("libusb returned null handle device"));
        }
        let mut owned = OwnedConfiguration {
            _api: &self.api,
            free: symbols.free,
            pointer: ptr::null_mut(),
        };
        // SAFETY: category8; checked SDK signature writes one owned descriptor tree, retained through copy.
        let code = unsafe { (symbols.get)(device, index, &mut owned.pointer) };
        self.api.checked(code, "libusb_get_config_descriptor")?;
        // SAFETY: category3; non-null libusb return points to a live SDK tree owned until this scope ends.
        let raw = unsafe { owned.pointer.as_ref() }
            .ok_or(Error::Contract("libusb returned null configuration"))?;
        // SAFETY: category10; libusb owns all descriptor arrays with the SDK-declared counts; copy checks null/count.
        unsafe { copy_configuration(raw) }
    }
}

/// Caller retains the SDK tree and its count-element nested arrays through this copy.
unsafe fn copy_configuration(raw: &RawConfiguration) -> Result<Configuration, Error> {
    // SAFETY: category10; caller guarantees SDK ownership/length; checked_slice rejects null nonempty arrays.
    let groups = unsafe { checked_slice(raw.interfaces, usize::from(raw.interfaces_count))? };
    let mut interfaces = Vec::new();
    for RawInterfaceGroup { alternates, count } in groups {
        let count =
            usize::try_from(*count).map_err(|_| Error::Contract("negative USB alternate count"))?;
        // SAFETY: category10; caller retains exactly count SDK alternate descriptors for this group.
        let alternates = unsafe { checked_slice(*alternates, count)? };
        for alternate in alternates {
            // SAFETY: category10; caller retains the declared endpoint array for this alternate.
            let endpoints = unsafe {
                checked_slice(alternate.endpoints, usize::from(alternate.endpoints_count))?
            };
            interfaces.push(InterfaceDescriptor {
                number: alternate.number,
                alternate: alternate.alternate,
                endpoints: endpoints
                    .iter()
                    .map(
                        |RawEndpoint {
                             address,
                             attributes,
                             packet_size,
                             ..
                         }| Endpoint {
                            address: *address,
                            attributes: *attributes,
                            packet_size: *packet_size,
                        },
                    )
                    .collect(),
            });
        }
    }
    Ok(Configuration {
        value: raw.value,
        interfaces,
    })
}

/// Caller supplies a retained SDK allocation containing count initialized T values.
unsafe fn checked_slice<'a, T>(pointer: *const T, count: usize) -> Result<&'a [T], Error> {
    if count == 0 {
        return Ok(&[]);
    }
    if pointer.is_null() || count > isize::MAX.unsigned_abs() / std::mem::size_of::<T>() {
        return Err(Error::Contract("invalid USB descriptor array"));
    }
    // SAFETY: categories3/6/10; caller guarantees live aligned SDK allocation; null and slice-size bounds checked.
    Ok(unsafe { slice::from_raw_parts(pointer, count) })
}
