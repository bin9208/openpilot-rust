use openpilot_msgq::{VisionFrame, VisionLayout, VisionMetadata};
use openpilot_startup_ui::{
    camera::PlaneFormat,
    egl::{Context, FrameLayout, OwnedImage},
};
use openpilot_ui_framework::{
    draw::{Draw, PixelBuffer, TextureResource},
    Error,
};
use std::{
    collections::HashMap,
    os::fd::{AsFd, OwnedFd},
    rc::Rc,
};

pub(super) struct Textures {
    pub frame: Option<VisionMetadata>,
    pub luma: Option<Box<dyn TextureResource>>,
    pub chroma: Option<Box<dyn TextureResource>>,
    egl: Option<Rc<Context>>,
    images: HashMap<usize, OwnedImage>,
    bytes: Vec<u8>,
    pending_image: Option<(usize, FrameLayout, OwnedFd)>,
}
fn error(error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::Io(std::io::Error::other(error))
}
fn integer(value: usize) -> Result<i32, Error> {
    i32::try_from(value).map_err(|_| Error::Contract("camera layout exceeds native dimensions"))
}
impl Textures {
    pub fn new(external: bool, draw: &mut dyn Draw) -> Result<Self, Error> {
        let egl = if external {
            Some(Rc::new(Context::new().map_err(error)?))
        } else {
            None
        };
        let luma = if external {
            Some(draw.upload_pixels(PixelBuffer {
                dimensions: (1, 1),
                rgba: &[0, 0, 0, 255],
            })?)
        } else {
            None
        };
        Ok(Self {
            frame: None,
            luma,
            chroma: None,
            egl,
            images: HashMap::new(),
            bytes: Vec::new(),
            pending_image: None,
        })
    }
    pub fn initialize(&mut self, layout: VisionLayout, draw: &mut dyn Draw) -> Result<(), Error> {
        self.images.clear();
        self.pending_image = None;
        if self.egl.is_none() {
            self.luma = Some(draw.camera_plane(
                (integer(layout.stride)?, integer(layout.height)?),
                PlaneFormat::Luma,
            )?);
            self.chroma = Some(draw.camera_plane(
                (integer(layout.stride / 2)?, integer(layout.height / 2)?),
                PlaneFormat::Chroma,
            )?);
            self.bytes.resize(layout.len, 0);
        }
        Ok(())
    }
    pub fn receive(&mut self, frame: &VisionFrame<'_>, draw: &mut dyn Draw) -> Result<(), Error> {
        let metadata = *frame.metadata();
        if self.egl.is_some() {
            self.pending_image = None;
            if !self.images.contains_key(&metadata.index) {
                let layout = FrameLayout {
                    width: integer(metadata.width)?,
                    height: integer(metadata.height)?,
                    stride: integer(metadata.stride)?,
                    uv_offset: integer(metadata.uv_offset)?,
                };
                self.pending_image =
                    Some((metadata.index, layout, frame.as_fd().try_clone_to_owned()?));
            }
        } else {
            self.bytes.resize(metadata.len, 0);
            frame.copy_into(&mut self.bytes).map_err(error)?;
            let luma = self
                .luma
                .as_ref()
                .ok_or(Error::Contract("camera luma not initialized"))?;
            let chroma = self
                .chroma
                .as_ref()
                .ok_or(Error::Contract("camera chroma not initialized"))?;
            let (y, uv) = self.bytes.split_at(metadata.uv_offset);
            draw.update_camera_plane(luma.id(), y)?;
            draw.update_camera_plane(chroma.id(), uv)?;
        }
        self.frame = Some(metadata);
        Ok(())
    }
    pub fn bind(&mut self, draw: &dyn Draw) -> Result<bool, Error> {
        let Some(frame) = self.frame else {
            return Ok(false);
        };
        if let Some(egl) = &self.egl {
            if let Some((index, layout, fd)) = &self.pending_image {
                if *index == frame.index {
                    match egl.create_owned(*layout, fd.as_fd()) {
                        Ok(image) => {
                            self.images.insert(*index, image);
                            self.pending_image = None;
                        }
                        Err(error) => {
                            eprintln!("Failed to create EGL image: {error}");
                            return Ok(false);
                        }
                    }
                }
            }
            let Some(image) = self.images.get(&frame.index) else {
                return Ok(false);
            };
            let texture = self
                .luma
                .as_ref()
                .ok_or(Error::Contract("external camera texture missing"))?;
            image.bind(draw.native_texture(texture.id())?);
        }
        Ok(true)
    }
    pub fn close(&mut self) {
        self.images.clear();
        self.pending_image = None;
        self.frame = None;
        self.luma = None;
        self.chroma = None;
        self.bytes.clear();
    }
}
