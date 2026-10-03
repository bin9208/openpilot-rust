use crate::{vision_bridge::ffi, Error, VisionLayout, VisionMetadata, VisionStream};
use std::{
    cell::RefCell,
    os::fd::{AsFd, BorrowedFd},
    rc::Rc,
};

pub struct VisionServer {
    owner: Rc<RefCell<cxx::UniquePtr<ffi::VisionPublisher>>>,
}

pub struct VisionImage {
    owner: Rc<RefCell<cxx::UniquePtr<ffi::VisionPublisher>>>,
    stream: i32,
    index: usize,
    fd: i32,
}

impl VisionServer {
    pub fn new(name: &str) -> Result<Self, Error> {
        Ok(Self {
            owner: Rc::new(RefCell::new(ffi::open_vision_server(name)?)),
        })
    }

    pub fn create_stream(
        &self,
        stream: VisionStream,
        count: usize,
        layout: VisionLayout,
    ) -> Result<Vec<VisionImage>, Error> {
        let mut owner = self.owner.borrow_mut();
        let stream = stream.native();
        let layout = ffi::ConnectionLayout {
            width: layout.width,
            height: layout.height,
            stride: layout.stride,
            uv_offset: layout.uv_offset,
            len: layout.len,
            available: true,
        };
        owner.pin_mut().create_stream(stream, count, &layout)?;
        let mut images = Vec::with_capacity(count);
        for index in 0..count {
            let fd = owner.pin_mut().descriptor(stream, index)?;
            images.push(VisionImage {
                owner: Rc::clone(&self.owner),
                stream,
                index,
                fd,
            });
        }
        Ok(images)
    }

    pub fn start_listener(&self) -> Result<(), Error> {
        Ok(self.owner.borrow_mut().pin_mut().start_listener()?)
    }
}

impl VisionImage {
    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        Ok(self.owner.borrow_mut().pin_mut().write_buffer(
            self.stream,
            self.index,
            offset,
            bytes,
        )?)
    }

    pub fn copy_into(&self, bytes: &mut [u8]) -> Result<(), Error> {
        Ok(self
            .owner
            .borrow_mut()
            .pin_mut()
            .copy_buffer(self.stream, self.index, bytes)?)
    }

    pub fn publish(&self, metadata: VisionMetadata) -> Result<(), Error> {
        Ok(self
            .owner
            .borrow_mut()
            .pin_mut()
            .publish(self.stream, self.index, &metadata)?)
    }
}

impl AsFd for VisionImage {
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: this image retains the server owning fd; no API removes or closes its buffers.
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}

pub struct RawVisionImage {
    owner: RefCell<cxx::UniquePtr<ffi::RawVisionBuffer>>,
    fd: i32,
    thread: std::marker::PhantomData<Rc<()>>,
}

impl RawVisionImage {
    pub fn new(length: usize) -> Result<Self, Error> {
        let owner = ffi::allocate_raw_vision(length)?;
        let fd = owner.descriptor();
        Ok(Self {
            owner: RefCell::new(owner),
            fd,
            thread: std::marker::PhantomData,
        })
    }

    pub fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        Ok(self
            .owner
            .borrow_mut()
            .pin_mut()
            .write_buffer(offset, bytes)?)
    }

    pub fn copy_into(&self, bytes: &mut [u8]) -> Result<(), Error> {
        Ok(self.owner.borrow().copy_buffer(bytes)?)
    }
}

impl AsFd for RawVisionImage {
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: native allocation owns this descriptor until this Rust owner is dropped.
        unsafe { BorrowedFd::borrow_raw(self.fd) }
    }
}
