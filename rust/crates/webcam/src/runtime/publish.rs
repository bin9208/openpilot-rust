use crate::{selection::CameraKind, Error};
use num_traits::ToPrimitive;
use openpilot_cereal::log_capnp::event;
use openpilot_messaging::runtime::PubMaster;
use openpilot_msgq::{
    UnalignedWebcamImage, VisionImage, VisionLayout, VisionMetadata, VisionServer, VisionStream,
};

pub(super) struct Publisher {
    images: Vec<VisionImage>,
    unaligned: Vec<UnalignedWebcamImage>,
    layout: VisionLayout,
}

#[derive(Clone, Copy)]
pub struct Publication<'a> {
    pub kind: CameraKind,
    pub frame_id: u32,
    pub bytes: &'a [u8],
}

impl Publisher {
    pub(super) fn new(
        server: &VisionServer,
        kind: CameraKind,
        width: f64,
        height: f64,
    ) -> Result<Self, Error> {
        let width = width
            .to_usize()
            .ok_or(Error::Contract("invalid capture width"))?;
        let height = height
            .to_usize()
            .ok_or(Error::Contract("invalid capture height"))?;
        let pixels = width
            .checked_mul(height)
            .ok_or(Error::Contract("camera extent overflow"))?;
        let length = pixels
            .checked_mul(3)
            .ok_or(Error::Contract("camera length overflow"))?
            / 2;
        let stream = match kind {
            CameraKind::Road => VisionStream::Road,
            CameraKind::WideRoad => VisionStream::WideRoad,
            CameraKind::Driver => VisionStream::Driver,
        };
        let layout = VisionLayout {
            width,
            height,
            stride: width,
            uv_offset: pixels,
            len: length,
        };
        let (images, unaligned) =
            if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
                server.create_inactive_webcam_stream(stream, 20, width, height)?;
                (Vec::new(), Vec::new())
            } else if !length.is_multiple_of(8) {
                (
                    Vec::new(),
                    server.create_unaligned_webcam_stream(stream, 20, layout)?,
                )
            } else {
                (server.create_stream(stream, 20, layout)?, Vec::new())
            };
        Ok(Self {
            images,
            unaligned,
            layout,
        })
    }

    pub(super) fn send(
        &self,
        master: &mut PubMaster,
        publication: Publication<'_>,
    ) -> Result<Vec<u8>, Error> {
        let count = self.images.len() + self.unaligned.len();
        if self.layout.len != publication.bytes.len() || count == 0 {
            return Err(Error::Contract(
                "captured NV12 length differs from advertised VisionIPC storage",
            ));
        }
        // Preserve Python's floating operation order before int truncation.
        let timestamp = (f64::from(publication.frame_id) * 0.05 * 1e9)
            .to_u64()
            .ok_or(Error::Contract("invalid webcam timestamp"))?;
        let index = usize::try_from(publication.frame_id)? % count;
        let metadata = VisionMetadata {
            frame_id: publication.frame_id,
            timestamp_sof: timestamp,
            timestamp_eof: timestamp,
            valid: false,
            width: self.layout.width,
            height: self.layout.height,
            stride: self.layout.stride,
            uv_offset: self.layout.uv_offset,
            len: self.layout.len,
            received: false,
            index,
            fd: -1,
        };
        if let Some(image) = self.images.get(index) {
            image.write(0, publication.bytes)?;
            image.publish(metadata)?;
        } else {
            self.unaligned[index].publish(publication.bytes, metadata)?;
        }
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder<'_>>();
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
        root.set_log_mono_time(
            u64::try_from(now.tv_sec)? * 1_000_000_000 + u64::try_from(now.tv_nsec)?,
        );
        root.set_valid(true);
        let mut frame = match publication.kind {
            CameraKind::Road => root.init_road_camera_state(),
            CameraKind::WideRoad => root.init_wide_road_camera_state(),
            CameraKind::Driver => root.init_driver_camera_state(),
        };
        frame.set_frame_id(publication.frame_id);
        let mut transform = frame.init_transform(9);
        for (index, value) in [1., 0., 0., 0., 1., 0., 0., 0., 1.].into_iter().enumerate() {
            transform.set(u32::try_from(index)?, value);
        }
        let bytes = capnp::serialize::write_message_to_words(&message);
        master.send(publication.kind.service(), &bytes)?;
        Ok(bytes)
    }
}
