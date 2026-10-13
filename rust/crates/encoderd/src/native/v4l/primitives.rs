#![allow(unsafe_code)]
mod buffer;
mod encoder;
mod events;
use super::{abi, io};
use crate::Error;
pub use buffer::{Allocator, Buffer, QueuedPlane};
pub use encoder::{EncoderControl, EncoderValue};
pub use events::{Event, Ready};
use std::os::fd::OwnedFd;

#[derive(Clone, Copy, Debug)]
pub enum Queue {
    Output,
    Capture,
}
impl Queue {
    pub(super) fn kind(self) -> u32 {
        match self {
            Self::Output => abi::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE,
            Self::Capture => abi::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatKind {
    H264,
    Nv12,
}
#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub size: u32,
    pub pixel_format: u32,
}
#[derive(Debug)]
pub struct Capability {
    pub driver: String,
    pub card: String,
}
pub struct Device {
    fd: OwnedFd,
}
impl Device {
    /// # Errors
    /// Preserves open failures from the supplied kernel device path.
    pub fn open(path: &str) -> Result<Self, Error> {
        Ok(Self {
            fd: io::open(path)?,
        })
    }
    /// # Errors
    /// Preserves QUERYCAP errors from the kernel.
    pub fn capability(&self) -> Result<Capability, Error> {
        let mut capability = abi::v4l2_capability::default();
        // SAFETY: QUERYCAP writes its matching initialized kernel ABI structure.
        unsafe {
            io::ioctl(&self.fd, abi::ENCODER_VIDIOC_QUERYCAP, &mut capability)?;
        }
        let text = |bytes: &[u8]| {
            String::from_utf8_lossy(bytes.split(|byte| *byte == 0).next().unwrap_or_default())
                .into_owned()
        };
        Ok(Capability {
            driver: text(&capability.driver),
            card: text(&capability.card),
        })
    }
    /// # Errors
    /// Preserves format request errors and validates ABI conversions.
    pub fn format(
        &self,
        queue: Queue,
        dimensions: (u32, u32),
        kind: FormatKind,
        get: bool,
    ) -> Result<Format, Error> {
        self.format_color(queue, dimensions, kind, get, 0)
    }
    /// # Errors
    /// Preserves the encoder format request with its source NV12 colorspace.
    pub fn encoder_format(
        &self,
        queue: Queue,
        dimensions: (u32, u32),
        kind: FormatKind,
    ) -> Result<Format, Error> {
        self.format_color(
            queue,
            dimensions,
            kind,
            false,
            if kind == FormatKind::Nv12 {
                abi::V4L2_COLORSPACE_470_SYSTEM_BG
            } else {
                abi::V4L2_COLORSPACE_DEFAULT
            },
        )
    }
    fn format_color(
        &self,
        queue: Queue,
        dimensions: (u32, u32),
        kind: FormatKind,
        get: bool,
        colorspace: u32,
    ) -> Result<Format, Error> {
        let mut format = abi::v4l2_format {
            type_: queue.kind(),
            ..Default::default()
        };
        format.fmt.pix_mp = abi::v4l2_pix_format_mplane {
            width: dimensions.0,
            height: dimensions.1,
            pixelformat: match kind {
                FormatKind::H264 => abi::ENCODER_V4L2_PIX_FMT_H264,
                FormatKind::Nv12 => abi::ENCODER_V4L2_PIX_FMT_NV12,
            },
            colorspace,
            ..Default::default()
        };
        // SAFETY: the active pix_mp member matches the requested multiplane queue.
        unsafe {
            io::ioctl(
                &self.fd,
                if get {
                    abi::ENCODER_VIDIOC_G_FMT
                } else {
                    abi::ENCODER_VIDIOC_S_FMT
                },
                &mut format,
            )?;
        }
        // SAFETY: the synchronous format request initialized this union member.
        let format = unsafe { format.fmt.pix_mp };
        Ok(Format {
            width: format.width,
            height: format.height,
            stride: format.plane_fmt[0].bytesperline,
            size: format.plane_fmt[0].sizeimage,
            pixel_format: format.pixelformat,
        })
    }
    /// # Errors
    /// Preserves the FPS request error from the kernel.
    pub fn fps(&self, queue: Queue, fps: u32) -> Result<(), Error> {
        let mut stream = abi::v4l2_streamparm {
            type_: queue.kind(),
            ..Default::default()
        };
        stream.parm.output = abi::v4l2_outputparm {
            timeperframe: abi::v4l2_fract {
                numerator: 1,
                denominator: fps,
            },
            ..Default::default()
        };
        // SAFETY: S_PARM synchronously reads the initialized output union member.
        unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_S_PARM, &mut stream) }
    }
    /// # Errors
    /// Preserves the decoder DPB control request error.
    pub fn decoder_dpb(&self) -> Result<(), Error> {
        let mut values = [
            abi::v4l2_ext_control {
                id: abi::V4L2_CID_MPEG_VIDC_VIDEO_STREAM_OUTPUT_MODE,
                ..Default::default()
            },
            abi::v4l2_ext_control {
                id: abi::V4L2_CID_MPEG_VIDC_VIDEO_DPB_COLOR_FORMAT,
                ..Default::default()
            },
        ];
        values[0].__bindgen_anon_1.value = 1;
        values[1].__bindgen_anon_1.value = 0;
        let mut controls = abi::v4l2_ext_controls {
            count: 2,
            controls: values.as_mut_ptr(),
            ..Default::default()
        };
        controls.__bindgen_anon_1.ctrl_class = abi::V4L2_CTRL_CLASS_MPEG;
        // SAFETY: both initialized controls remain live throughout S_EXT_CTRLS.
        unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_S_EXT_CTRLS, &mut controls) }
    }
    /// # Errors
    /// Preserves REQBUFS errors while returning the driver's actual count.
    pub fn buffers(&self, queue: Queue, count: u32) -> Result<u32, Error> {
        let mut buffers = abi::v4l2_requestbuffers {
            count,
            type_: queue.kind(),
            memory: abi::V4L2_MEMORY_USERPTR,
            ..Default::default()
        };
        // SAFETY: REQBUFS uses its initialized matching ABI structure.
        unsafe {
            io::ioctl(&self.fd, abi::ENCODER_VIDIOC_REQBUFS, &mut buffers)?;
        }
        Ok(buffers.count)
    }
    /// # Errors
    /// Preserves STREAMON/OFF errors for the selected queue.
    pub fn stream(&self, queue: Queue, on: bool) -> Result<(), Error> {
        io::stream(&self.fd, queue.kind(), on)
    }
    #[must_use]
    pub fn minimum_capture_buffers(&self) -> Option<u32> {
        use std::os::fd::AsRawFd;
        let mut control = abi::v4l2_control {
            id: abi::V4L2_CID_MIN_BUFFERS_FOR_CAPTURE,
            ..Default::default()
        };
        // SAFETY: the source deliberately performs one G_CTRL attempt, without EINTR retry.
        let result = unsafe {
            libc::ioctl(
                self.fd.as_raw_fd(),
                abi::ENCODER_VIDIOC_G_CTRL,
                &mut control,
            )
        };
        (result == 0)
            .then(|| u32::try_from(control.value).ok().filter(|value| *value > 0))
            .flatten()
    }
    /// # Errors
    /// Preserves dequeue errors other than the source's EAGAIN/ENOENT absence.
    pub fn dequeue(&self, queue: Queue) -> Result<Option<Dequeued>, Error> {
        self.dequeue_absent(queue, true)
    }
    /// # Errors
    /// Preserves encoder dequeue errors except the source's EAGAIN absence.
    pub fn encoder_dequeue(&self, queue: Queue) -> Result<Option<Dequeued>, Error> {
        self.dequeue_absent(queue, false)
    }
    fn dequeue_absent(&self, queue: Queue, allow_enoent: bool) -> Result<Option<Dequeued>, Error> {
        let mut plane = abi::v4l2_plane::default();
        let mut value = abi::v4l2_buffer {
            type_: queue.kind(),
            memory: abi::V4L2_MEMORY_USERPTR,
            length: 1,
            ..Default::default()
        };
        value.m.planes = &mut plane;
        // SAFETY: the live plane receives the matching DQBUF structure's nested result.
        if let Err(error) = unsafe { io::ioctl(&self.fd, abi::ENCODER_VIDIOC_DQBUF, &mut value) } {
            return match error {
                Error::Io(error)
                    if error.raw_os_error() == Some(libc::EAGAIN)
                        || (allow_enoent && error.raw_os_error() == Some(libc::ENOENT)) =>
                {
                    Ok(None)
                }
                error => Err(error),
            };
        }
        let seconds = u64::from_ne_bytes(value.timestamp.tv_sec.to_ne_bytes());
        let micros = u64::from_ne_bytes(value.timestamp.tv_usec.to_ne_bytes());
        Ok(Some(Dequeued {
            index: usize::try_from(value.index)?,
            used: usize::try_from(plane.bytesused)?,
            sequence: seconds.wrapping_mul(1_000_000).wrapping_add(micros),
            flags: value.flags,
        }))
    }
}
#[derive(Debug)]
pub struct Dequeued {
    pub index: usize,
    pub used: usize,
    pub sequence: u64,
    pub flags: u32,
}
