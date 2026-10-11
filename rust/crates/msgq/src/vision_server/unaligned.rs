use super::{State, VisionServer};
use crate::{vision_wire, Error, VisionLayout, VisionMetadata, VisionStream};
use std::{cell::RefCell, fs::File, os::unix::fs::FileExt, rc::Rc};

/// Webcam-only producer storage whose tail ID is not naturally aligned.
/// No mapping or typed frame-ID pointer is ever constructed.
pub struct UnalignedWebcamImage {
    owner: Rc<RefCell<State>>,
    file: Rc<File>,
    layout: VisionLayout,
    index: usize,
    stream: VisionStream,
}

impl VisionServer {
    /// Allocate a source-admitted even webcam layout with an unaligned tail.
    /// Ordinary mapped streams and client alignment checks stay unchanged.
    pub fn create_unaligned_webcam_stream(
        &self,
        stream: VisionStream,
        count: usize,
        layout: VisionLayout,
    ) -> Result<Vec<UnalignedWebcamImage>, Error> {
        let pixels = layout
            .width
            .checked_mul(layout.height)
            .ok_or(Error::Invalid("webcam extent overflow"))?;
        let length = pixels
            .checked_mul(3)
            .ok_or(Error::Invalid("webcam length overflow"))?
            / 2;
        if layout.stride != layout.width
            || layout.uv_offset != pixels
            || layout.len != length
            || length.is_multiple_of(8)
        {
            return Err(Error::Invalid(
                "unaligned webcam requires original packed even layout",
            ));
        }
        layout.validate(
            length
                .checked_add(8)
                .ok_or(Error::Invalid("webcam length overflow"))?,
        )?;
        let files = self.create_webcam_file_stream(stream, count, layout)?;
        Ok(files
            .into_iter()
            .enumerate()
            .map(|(index, file)| UnalignedWebcamImage {
                owner: Rc::clone(&self.owner),
                file,
                layout,
                index,
                stream,
            })
            .collect())
    }
}

impl UnalignedWebcamImage {
    /// Store exact payload then native-endian ID bytes before queue publication.
    pub fn publish(&self, bytes: &[u8], metadata: VisionMetadata) -> Result<(), Error> {
        if bytes.len() != self.layout.len {
            return Err(Error::Invalid("unaligned webcam payload length mismatch"));
        }
        self.file
            .write_all_at(bytes, 0)
            .map_err(|error| Error::Io("write webcam file payload", error))?;
        self.file
            .write_all_at(
                &u64::from(metadata.frame_id).to_ne_bytes(),
                u64::try_from(self.layout.len)
                    .map_err(|_| Error::Invalid("webcam ID offset overflow"))?,
            )
            .map_err(|error| Error::Io("write webcam file frame ID", error))?;
        let mut state = self.owner.borrow_mut();
        let packet = vision_wire::encode_packet(state.server_id, self.index, metadata)?;
        let stream = state.streams[usize::try_from(self.stream.native())
            .map_err(|_| Error::Invalid("webcam stream index overflow"))?]
        .as_mut()
        .ok_or(Error::Invalid("webcam file stream absent"))?;
        stream.publisher.send(&packet)
    }
}
