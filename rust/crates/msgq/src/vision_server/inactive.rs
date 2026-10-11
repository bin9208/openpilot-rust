use super::{Stream, Transfer, VisionServer};
use crate::{
    queue::{Kind, Namespace, PublisherMode, Queue},
    vision_memory::allocate_fd,
    vision_wire, Error, VisionLayout, VisionStream,
};
use std::{
    fs::File,
    os::fd::{AsFd, AsRawFd},
    rc::Rc,
};

pub(super) struct FileStorage {
    layout: VisionLayout,
    stream: VisionStream,
    files: Vec<Rc<File>>,
}

impl FileStorage {
    pub(super) fn transfer(&self, output: &mut Transfer, server_id: u64) -> Result<(), Error> {
        let mapped_length = self
            .layout
            .len
            .checked_add(8)
            .ok_or(Error::Invalid("inactive buffer size overflow"))?;
        for (index, file) in self.files.iter().enumerate() {
            // File descriptors contain no server-side mapping or frame-ID
            // pointer. The original client remaps each received FD itself.
            output
                .payload
                .extend_from_slice(&vision_wire::encode_buffer(
                    vision_wire::Buffer {
                        layout: self.layout,
                        mapped_length,
                        server_id,
                        index,
                        stream: self.stream,
                    },
                    0,
                    file.as_raw_fd(),
                )?);
            output.descriptors.push(
                file.as_fd()
                    .try_clone_to_owned()
                    .map_err(|error| Error::Io("duplicate inactive VisionIPC descriptor", error))?,
            );
        }
        Ok(())
    }
}

impl VisionServer {
    /// Advertise the original webcam's zero/odd layout without a publishing API.
    ///
    /// Storage is zero-filled file data with no frame-ID accesses or mapping.
    /// Ordinary streams and clients retain their existing layout validation.
    ///
    /// # Errors
    /// Rejects a normal even layout, duplicate/late stream creation, invalid
    /// buffer counts, overflowing extents and file or queue allocation errors.
    pub fn create_inactive_webcam_stream(
        &self,
        stream: VisionStream,
        count: usize,
        width: usize,
        height: usize,
    ) -> Result<(), Error> {
        if width != 0 && height != 0 && width.is_multiple_of(2) && height.is_multiple_of(2) {
            return Err(Error::Invalid(
                "inactive webcam stream must have a zero/odd extent",
            ));
        }
        let pixels = width
            .checked_mul(height)
            .ok_or(Error::Invalid("inactive camera extent overflow"))?;
        let len = pixels
            .checked_mul(3)
            .ok_or(Error::Invalid("inactive camera length overflow"))?
            / 2;
        let layout = VisionLayout {
            width,
            height,
            stride: width,
            uv_offset: pixels,
            len,
        };
        self.create_webcam_file_stream(stream, count, layout)?;
        Ok(())
    }

    pub(super) fn create_webcam_file_stream(
        &self,
        stream: VisionStream,
        count: usize,
        layout: VisionLayout,
    ) -> Result<Vec<Rc<File>>, Error> {
        let mut state = self.owner.borrow_mut();
        let index =
            usize::try_from(stream.native()).map_err(|_| Error::Invalid("invalid stream index"))?;
        if state.listener.is_some()
            || state.streams[index].is_some()
            || count == 0
            || count >= vision_wire::MAX_FDS
        {
            return Err(Error::Invalid("invalid webcam file stream creation"));
        }
        let mapped_length = layout
            .len
            .checked_add(8)
            .filter(|value| isize::try_from(*value).is_ok())
            .ok_or(Error::Invalid("webcam file buffer size overflow"))?;
        let files = (0..count)
            .map(|_| allocate_fd(mapped_length).map(Rc::new))
            .collect::<Result<Vec<_>, _>>()?;
        let publisher = Queue::open(
            &format!("visionipc_{}_{}", state.name, stream.native()),
            Kind::Publisher(PublisherMode::Original),
            1024 * 1024,
            Namespace::Runtime,
        )?;
        state.streams[index] = Some(Stream {
            publisher,
            buffers: Vec::new(),
            webcam_files: Some(FileStorage {
                layout,
                stream,
                files: files.clone(),
            }),
        });
        Ok(files)
    }
}
