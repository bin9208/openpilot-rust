use crate::Error;
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd},
        unix::fs::OpenOptionsExt,
    },
    sync::{Arc, Mutex},
};

#[repr(C)]
#[derive(Default)]
struct Allocation {
    len: usize,
    align: usize,
    heap_id_mask: u32,
    flags: u32,
    handle: i32,
}

#[repr(C)]
#[derive(Default)]
struct Descriptor {
    handle: i32,
    fd: i32,
}

#[repr(C)]
struct Handle {
    handle: i32,
}

#[repr(C)]
struct Custom {
    command: u32,
    argument: libc::c_ulong,
}

#[repr(C)]
struct Flush {
    handle: i32,
    fd: i32,
    address: *mut libc::c_void,
    offset: u32,
    length: u32,
}

const fn request<T>(number: u32) -> libc::c_ulong {
    ((3 << 30) | ((std::mem::size_of::<T>() as u32) << 16) | (0x49 << 8) | number) as libc::c_ulong
}

const fn cache_request(from_device: bool) -> u32 {
    (3 << 30) | ((std::mem::size_of::<Flush>() as u32) << 16) | (0x4d << 8) | from_device as u32
}

fn ioctl<T>(
    device: &File,
    request: libc::c_ulong,
    argument: &mut T,
    operation: &'static str,
) -> Result<(), Error> {
    for attempt in 0..=100 {
        // SAFETY: callers pair each request with its exact repr(C) kernel layout;
        // the live writable argument and device FD remain owned throughout ioctl.
        let result =
            unsafe { libc::ioctl(device.as_raw_fd(), request, std::ptr::from_mut(argument)) };
        if result == 0 {
            return Ok(());
        }
        if result != -1 {
            return Err(Error::Corrupt(
                "ION ioctl returned an unexpected nonzero status",
            ));
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted || attempt == 100 {
            return Err(Error::Io(operation, error));
        }
    }
    Err(Error::Invalid("ION retry state exhausted"))
}

fn device() -> Result<Arc<File>, Error> {
    static DEVICE: Mutex<Option<Arc<File>>> = Mutex::new(None);
    let mut device = DEVICE
        .lock()
        .map_err(|_| Error::Invalid("ION device lock poisoned"))?;
    if let Some(device) = device.as_ref() {
        return Ok(Arc::clone(device));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open("/dev/ion")
        .map_err(|error| Error::Io("open ION device", error))?;
    let file = Arc::new(file);
    *device = Some(Arc::clone(&file));
    Ok(file)
}

pub(crate) struct IonHandle {
    device: Arc<File>,
    handle: i32,
}

impl IonHandle {
    pub(crate) fn allocate(length: usize) -> Result<(OwnedFd, Self), Error> {
        let device = device()?;
        let mut allocation = Allocation {
            len: length,
            align: 4096,
            heap_id_mask: 1 << 25,
            flags: 1,
            handle: 0,
        };
        ioctl(
            &device,
            request::<Allocation>(0),
            &mut allocation,
            "allocate ION buffer",
        )?;
        let handle = Self {
            device,
            handle: allocation.handle,
        };
        let mut descriptor = Descriptor {
            handle: handle.handle,
            fd: -1,
        };
        ioctl(
            &handle.device,
            request::<Descriptor>(4),
            &mut descriptor,
            "share ION buffer",
        )?;
        if descriptor.fd < 0 {
            return Err(Error::Corrupt("ION returned an invalid descriptor"));
        }
        // SAFETY: successful ION_IOC_SHARE created and transferred this fresh FD.
        Ok((unsafe { OwnedFd::from_raw_fd(descriptor.fd) }, handle))
    }

    pub(crate) fn import(fd: BorrowedFd<'_>) -> Result<Self, Error> {
        let device = device()?;
        let mut descriptor = Descriptor {
            handle: 0,
            fd: fd.as_raw_fd(),
        };
        ioctl(
            &device,
            request::<Descriptor>(5),
            &mut descriptor,
            "import ION buffer",
        )?;
        Ok(Self {
            device,
            handle: descriptor.handle,
        })
    }

    pub(crate) fn sync(
        &self,
        address: usize,
        length: usize,
        from_device: bool,
    ) -> Result<(), Error> {
        let mut flush = Flush {
            handle: self.handle,
            fd: 0,
            address: std::ptr::with_exposed_provenance_mut(address),
            offset: 0,
            length: length as u32,
        };
        let command = cache_request(from_device);
        let mut custom = Custom {
            command,
            argument: std::ptr::from_mut(&mut flush).expose_provenance() as libc::c_ulong,
        };
        ioctl(
            &self.device,
            request::<Custom>(6),
            &mut custom,
            "synchronize ION cache",
        )
    }
}

impl Drop for IonHandle {
    fn drop(&mut self) {
        if let Err(error) = ioctl(
            &self.device,
            request::<Handle>(1),
            &mut Handle {
                handle: self.handle,
            },
            "free ION buffer",
        ) {
            eprintln!("{error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn ion_layouts_and_requests_match_original_kernel_headers() {
        let oracle = crate::abi_tests::layout();
        for (name, value) in [
            ("ion_allocation_data", size_of::<Allocation>()),
            ("ion_fd_data", size_of::<Descriptor>()),
            ("ion_handle_data", size_of::<Handle>()),
            ("ion_custom_data", size_of::<Custom>()),
            ("ion_flush_data", size_of::<Flush>()),
            ("ion_allocation_data.len", offset_of!(Allocation, len)),
            ("ion_allocation_data.align", offset_of!(Allocation, align)),
            (
                "ion_allocation_data.heap_id_mask",
                offset_of!(Allocation, heap_id_mask),
            ),
            ("ion_allocation_data.flags", offset_of!(Allocation, flags)),
            ("ion_allocation_data.handle", offset_of!(Allocation, handle)),
            ("ion_fd_data.handle", offset_of!(Descriptor, handle)),
            ("ion_fd_data.fd", offset_of!(Descriptor, fd)),
            ("ion_custom_data.cmd", offset_of!(Custom, command)),
            ("ion_custom_data.arg", offset_of!(Custom, argument)),
            ("ion_flush_data.handle", offset_of!(Flush, handle)),
            ("ion_flush_data.fd", offset_of!(Flush, fd)),
            ("ion_flush_data.vaddr", offset_of!(Flush, address)),
            ("ion_flush_data.offset", offset_of!(Flush, offset)),
            ("ion_flush_data.length", offset_of!(Flush, length)),
        ] {
            assert_eq!(oracle[name], value as u64, "{name}");
        }
        for (name, value) in [
            ("ION_IOC_ALLOC", request::<Allocation>(0)),
            ("ION_IOC_FREE", request::<Handle>(1)),
            ("ION_IOC_SHARE", request::<Descriptor>(4)),
            ("ION_IOC_IMPORT", request::<Descriptor>(5)),
            ("ION_IOC_CUSTOM", request::<Custom>(6)),
            ("ION_IOC_CLEAN_CACHES", u64::from(cache_request(false))),
            ("ION_IOC_INV_CACHES", u64::from(cache_request(true))),
        ] {
            assert_eq!(oracle[name], value, "{name}");
        }
    }
}
