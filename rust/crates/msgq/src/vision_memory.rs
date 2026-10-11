use crate::{mapped::ImageMapping, vision_wire, Error};
use std::{
    fs::File,
    os::fd::{AsFd, OwnedFd},
};

pub(crate) struct Buffer {
    pub mapping: ImageMapping,
    pub fd: File,
    #[cfg(feature = "visionipc-ion")]
    ion: crate::ion::IonHandle,
    pub wire: vision_wire::Buffer,
}

fn length(length: usize) -> Result<usize, Error> {
    if length == 0 || !length.is_multiple_of(8) {
        return Err(Error::Invalid(
            "VisionIPC allocation length must be nonzero and 8-byte aligned",
        ));
    }
    length
        .checked_add(8)
        .filter(|value| *value <= isize::MAX as usize)
        .ok_or(Error::Invalid("VisionIPC allocation length overflow"))
}

#[cfg(any(not(feature = "visionipc-ion"), feature = "webcam-inactive-stream"))]
pub(crate) fn allocate_fd(length: usize) -> Result<File, Error> {
    use std::{
        fs::OpenOptions,
        os::unix::fs::OpenOptionsExt,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static INDEX: AtomicUsize = AtomicUsize::new(0);
    let path = format!(
        "/dev/shm/msgq_visionbuf_{}_{}",
        std::process::id(),
        INDEX.fetch_add(1, Ordering::Relaxed)
    );
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o664)
        .open(&path)
        .map_err(|error| Error::Io("allocate VisionIPC buffer", error))?;
    std::fs::remove_file(path).map_err(|error| Error::Io("unlink VisionIPC buffer", error))?;
    file.set_len(
        u64::try_from(length).map_err(|_| Error::Invalid("VisionIPC file length overflow"))?,
    )
    .map_err(|error| Error::Io("size VisionIPC buffer", error))?;
    Ok(file)
}

impl Buffer {
    pub(crate) fn allocate(wire: vision_wire::Buffer) -> Result<Self, Error> {
        let mapped_length = length(wire.layout.len)?;
        #[cfg(not(feature = "visionipc-ion"))]
        let fd = allocate_fd(mapped_length)?;
        #[cfg(feature = "visionipc-ion")]
        let (fd, ion) = crate::ion::IonHandle::allocate(mapped_length)?;
        #[cfg(feature = "visionipc-ion")]
        let fd = File::from(fd);
        let mapping = ImageMapping::new(fd.as_fd(), wire.layout.len, mapped_length)?;
        #[cfg(feature = "visionipc-ion")]
        {
            let zeros = [0; 4096];
            for offset in (0..wire.layout.len).step_by(zeros.len()) {
                mapping.write(offset, &zeros[..zeros.len().min(wire.layout.len - offset)])?;
            }
            mapping.set_frame_id(0)?;
        }
        Ok(Self {
            mapping,
            fd,
            #[cfg(feature = "visionipc-ion")]
            ion,
            wire,
        })
    }

    pub(crate) fn import(wire: vision_wire::Buffer, fd: OwnedFd) -> Result<Self, Error> {
        let fd = File::from(fd);
        #[cfg(not(feature = "visionipc-ion"))]
        {
            let metadata = fd
                .metadata()
                .map_err(|error| Error::Io("inspect VisionIPC buffer", error))?;
            if !metadata.is_file() || metadata.len() < wire.mapped_length as u64 {
                return Err(Error::Corrupt(
                    "VisionIPC descriptor is not a sufficiently sized shared buffer",
                ));
            }
        }
        #[cfg(feature = "visionipc-ion")]
        let ion = crate::ion::IonHandle::import(fd.as_fd())?;
        let mapping = ImageMapping::new(fd.as_fd(), wire.layout.len, wire.mapped_length)?;
        Ok(Self {
            mapping,
            fd,
            #[cfg(feature = "visionipc-ion")]
            ion,
            wire,
        })
    }

    pub(crate) fn sync(&self, from_device: bool) {
        #[cfg(feature = "visionipc-ion")]
        if let Err(error) = self
            .ion
            .sync(self.mapping.address(), self.wire.layout.len, from_device)
        {
            eprintln!("{error}");
        }
        #[cfg(not(feature = "visionipc-ion"))]
        let _ = from_device;
    }
}
