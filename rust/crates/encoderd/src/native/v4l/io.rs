#![allow(unsafe_code)]
use super::abi;
use crate::{native::Mapping, Error};
use std::{
    fs::OpenOptions,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::fs::OpenOptionsExt,
    },
    sync::Arc,
};

pub fn open(path: &str) -> Result<OwnedFd, Error> {
    for attempt in 0..=100 {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => return Ok(file.into()),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted && attempt < 100 => {}
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}

/// The caller must match the generated request to T and keep any nested pointers
/// valid for the synchronous kernel call. Only this module's typed wrappers and
/// the encoder setup code may issue requests.
pub unsafe fn ioctl<T>(fd: &OwnedFd, request: u64, value: &mut T) -> Result<(), Error> {
    let ion_request = matches!(
        request,
        abi::ENCODER_ION_IOC_ALLOC
            | abi::ENCODER_ION_IOC_SHARE
            | abi::ENCODER_ION_IOC_FREE
            | abi::ENCODER_ION_IOC_CUSTOM
    );
    let mut interrupted = 0;
    loop {
        // SAFETY: [FFI boundary] callers pair the generated kernel request and
        // its C-layout argument, including live nested buffers where required.
        let result =
            unsafe { libc::ioctl(fd.as_raw_fd(), request as libc::c_ulong, value as *mut T) };
        if result >= 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted || (ion_request && interrupted == 100) {
            return Err(error.into());
        }
        if ion_request {
            interrupted += 1;
        }
    }
}

pub struct IonBuffer {
    // Mapping is explicitly dropped before releasing the ION allocation handle.
    mapping: Option<Mapping>,
    ion: Arc<OwnedFd>,
    handle: abi::ion_user_handle_t,
    length: usize,
}
impl IonBuffer {
    pub fn allocate(ion: Arc<OwnedFd>, length: usize) -> Result<Self, Error> {
        let mut allocation = abi::ion_allocation_data {
            len: length
                .checked_add(8)
                .ok_or(Error::Contract("ION allocation overflow"))?,
            align: 4096,
            heap_id_mask: 1 << abi::ENCODER_ION_IOMMU_HEAP_ID,
            flags: abi::ION_FLAG_CACHED,
            ..Default::default()
        };
        // SAFETY: [FFI boundary] ALLOC reads/writes its fully initialized ABI structure.
        unsafe {
            ioctl(&ion, abi::ENCODER_ION_IOC_ALLOC, &mut allocation)?;
        }
        let mut owner = Self {
            mapping: None,
            ion,
            handle: allocation.handle,
            length,
        };
        let mut shared = abi::ion_fd_data {
            handle: owner.handle,
            fd: -1,
        };
        // SAFETY: [FFI boundary] SHARE produces one new owned FD for the live handle.
        unsafe {
            ioctl(&owner.ion, abi::ENCODER_ION_IOC_SHARE, &mut shared)?;
        }
        if shared.fd < 0 {
            return Err(Error::Contract("ION shared an invalid FD"));
        }
        // SAFETY: [double free / ownership] SHARE returned a new owned FD; no
        // other owner closes it. The allocation stays pinned by this object.
        let fd = unsafe { OwnedFd::from_raw_fd(shared.fd) };
        // SAFETY: [FFI boundary] the successful allocation covers this exact size
        // and remains live until the mapping is dropped in this owner's Drop.
        let mapping = unsafe { Mapping::new(fd, allocation.len)? };
        // SAFETY: [bounds/aliasing] this freshly allocated buffer is not queued
        // to the driver yet, and the range is its full initialized allocation.
        unsafe {
            std::ptr::write_bytes(mapping.address() as *mut u8, 0, mapping.length());
        }
        owner.mapping = Some(mapping);
        Ok(owner)
    }
    pub fn mapping(&self) -> &Mapping {
        self.mapping.as_ref().expect("live ION mapping")
    }
    pub fn length(&self) -> usize {
        self.length
    }
    pub fn sync_from_device(&self) -> Result<(), Error> {
        let mut flush = abi::ion_flush_data {
            handle: self.handle,
            fd: 0,
            vaddr: self.mapping().address() as *mut _,
            offset: 0,
            length: u32::try_from(self.length)?,
        };
        let mut custom = abi::ion_custom_data {
            cmd: u32::try_from(abi::ENCODER_ION_IOC_INV_CACHES)?,
            arg: (&mut flush as *mut abi::ion_flush_data) as libc::c_ulong,
        };
        // SAFETY: [FFI boundary] CUSTOM embeds a live flush structure whose
        // address and length identify this retained ION allocation.
        unsafe { ioctl(&self.ion, abi::ENCODER_ION_IOC_CUSTOM, &mut custom) }
    }
}
impl Drop for IonBuffer {
    fn drop(&mut self) {
        drop(self.mapping.take());
        let mut handle = abi::ion_handle_data {
            handle: self.handle,
        };
        // SAFETY: [double free / ownership] this sole handle owner frees it once,
        // after all mappings and submitted driver operations have ended.
        if let Err(error) = unsafe { ioctl(&self.ion, abi::ENCODER_ION_IOC_FREE, &mut handle) } {
            super::warn(format!("Failed to free buffer: {error}"));
        }
    }
}

pub fn request_buffers(fd: &OwnedFd, kind: u32, count: u32) -> Result<(), Error> {
    let mut value = abi::v4l2_requestbuffers {
        count,
        type_: kind,
        memory: abi::V4L2_MEMORY_USERPTR,
        ..Default::default()
    };
    // SAFETY: [FFI boundary] REQBUFS uses its matching initialized ABI type.
    unsafe { ioctl(fd, abi::ENCODER_VIDIOC_REQBUFS, &mut value) }
}
pub fn stream(fd: &OwnedFd, kind: u32, on: bool) -> Result<(), Error> {
    let mut kind = kind;
    // SAFETY: [FFI boundary] STREAMON/OFF accept a pointer to the queue enum.
    unsafe {
        ioctl(
            fd,
            if on {
                abi::ENCODER_VIDIOC_STREAMON
            } else {
                abi::ENCODER_VIDIOC_STREAMOFF
            },
            &mut kind,
        )
    }
}
pub fn queue(
    fd: &OwnedFd,
    kind: u32,
    index: u32,
    mapping: &Mapping,
    length: usize,
    timestamp_us: u64,
) -> Result<(), Error> {
    if length > mapping.length() {
        return Err(Error::Contract("V4L queue exceeds retained mapping"));
    }
    let mut plane = abi::v4l2_plane {
        bytesused: u32::try_from(length)?,
        length: u32::try_from(length)?,
        ..Default::default()
    };
    plane.m.userptr = mapping.address() as libc::c_ulong;
    plane.reserved[0] = u32::try_from(mapping.fd().as_raw_fd())?;
    let mut buffer = abi::v4l2_buffer {
        index,
        type_: kind,
        flags: abi::V4L2_BUF_FLAG_TIMESTAMP_COPY,
        memory: abi::V4L2_MEMORY_USERPTR,
        length: 1,
        ..Default::default()
    };
    buffer.timestamp.tv_sec = (timestamp_us / 1_000_000).try_into()?;
    buffer.timestamp.tv_usec = (timestamp_us % 1_000_000).try_into()?;
    buffer.m.planes = &mut plane;
    // SAFETY: [FFI boundary / lifetime] plane is live for the ioctl; the caller
    // retains the backing mapping until this queue slot is dequeued or stopped.
    unsafe { ioctl(fd, abi::ENCODER_VIDIOC_QBUF, &mut buffer) }
}
pub struct Dequeued {
    pub index: usize,
    pub length: usize,
    pub flags: u32,
    pub timestamp_us: u64,
}
pub fn dequeue(fd: &OwnedFd, kind: u32) -> Result<Dequeued, Error> {
    let mut plane = abi::v4l2_plane::default();
    let mut buffer = abi::v4l2_buffer {
        type_: kind,
        memory: abi::V4L2_MEMORY_USERPTR,
        length: 1,
        ..Default::default()
    };
    buffer.m.planes = &mut plane;
    // SAFETY: [FFI boundary] one initialized plane is valid for this synchronous
    // request; the driver-written index and byte count are checked by consumers.
    unsafe {
        ioctl(fd, abi::ENCODER_VIDIOC_DQBUF, &mut buffer)?;
    }
    if plane.data_offset != 0 {
        return Err(Error::Contract("source V4L zero data-offset assertion"));
    }
    Ok(Dequeued {
        index: buffer.index.try_into()?,
        length: plane.bytesused.try_into()?,
        flags: buffer.flags,
        timestamp_us: u64::try_from(buffer.timestamp.tv_sec)?
            .checked_mul(1_000_000)
            .and_then(|seconds| {
                u64::try_from(buffer.timestamp.tv_usec)
                    .ok()
                    .and_then(|micros| seconds.checked_add(micros))
            })
            .ok_or(Error::Contract("V4L timestamp overflow"))?,
    })
}
