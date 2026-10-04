use super::super::{platform, shared::Shared, Error};
use crate::{
    nv12::{Frame, Layout},
    service::Stream,
};
use openpilot_jpeg::{Color, Layout as JpegLayout, Options, Quality};
use openpilot_msgq::{VisionClient, VisionStream};
use std::{sync::Arc, time::Duration};

pub struct Camera {
    client: Option<VisionClient>,
    pub bytes: Vec<u8>,
    pub layout: Layout,
    stream: Stream,
}

impl Camera {
    pub const fn new(stream: Stream) -> Self {
        Self {
            client: None,
            bytes: Vec::new(),
            layout: Layout {
                width: 0,
                height: 0,
                stride: 0,
                uv_offset: 0,
            },
            stream,
        }
    }

    pub fn receive(&mut self, shared: &Shared) -> Result<bool, Error> {
        if self
            .client
            .as_ref()
            .is_none_or(|client| !client.is_connected())
        {
            let stream = match self.stream {
                Stream::Wide => VisionStream::WideRoad,
                Stream::Road => VisionStream::Road,
            };
            let mut client = VisionClient::new("camerad", stream, true)?;
            if !client.connect()? {
                shared.state()?.cameras[self.stream.index()].error = match self.stream {
                    Stream::Wide => {
                        "wide road camera is unavailable; start openpilot/camerad first"
                    }
                    Stream::Road => "road camera is unavailable; start openpilot/camerad first",
                }
                .to_owned();
                platform::sleep(1.0)?;
                self.client = Some(client);
                return Ok(false);
            }
            self.client = Some(client);
        }
        let client = self
            .client
            .as_mut()
            .ok_or(Error::Contract("camera client missing"))?;
        let Some(frame) = client.receive(Duration::ZERO)? else {
            platform::sleep(0.005)?;
            return Ok(false);
        };
        let metadata = frame.metadata();
        self.layout = Layout {
            width: metadata.width,
            height: metadata.height,
            stride: metadata.stride,
            uv_offset: metadata.uv_offset,
        };
        self.bytes.resize(metadata.len, 0);
        frame.copy_into(&mut self.bytes)?;
        Ok(true)
    }

    pub fn frame(&self) -> Result<Frame<'_>, Error> {
        Ok(Frame::new(&self.bytes, self.layout)?)
    }

    pub fn snapshot(&self) -> Result<Arc<[u8]>, Error> {
        let frame = self.frame()?;
        let (image, color, quality) = match self.stream {
            Stream::Road => (crate::inference::lane_image(&frame)?, Color::Gray, 50),
            Stream::Wide => {
                let dimensions = openpilot_opencv_runtime::Dimensions::new(
                    u32::try_from(self.layout.width)
                        .map_err(|_| Error::Contract("snapshot width overflow"))?,
                    u32::try_from(self.layout.height)
                        .map_err(|_| Error::Contract("snapshot height overflow"))?,
                )?;
                (
                    openpilot_opencv_runtime::nv12_to_rgb(&frame.pack()?, dimensions)?,
                    Color::Rgb,
                    85,
                )
            }
        };
        let view = image.view();
        let dimensions = view.layout().dimensions();
        Ok(openpilot_jpeg::encode_with(
            view.data(),
            JpegLayout::new(dimensions.width(), dimensions.height(), color)?,
            Options::new(Quality::new(quality)?),
        )?
        .into())
    }

    pub fn recover(&mut self, error: Error, shared: &Shared) -> Result<(), Error> {
        match error {
            Error::Io(_)
            | Error::Transport(openpilot_msgq::Error::Io(_, _))
            | Error::Nv12(_)
            | Error::OpenCv(_)
            | Error::Inference(_)
            | Error::Jpeg(_)
            | Error::JpegContract(_)
            | Error::Policy(crate::Error::Invalid(_)) => {
                shared.state()?.cameras[self.stream.index()].error = error.to_string();
                self.client = None;
                platform::sleep(1.0)
            }
            error => Err(error),
        }
    }
}
