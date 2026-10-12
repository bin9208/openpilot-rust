use super::{input::Input, Error};
use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    camera::{CameraDraw, Style},
    egl::{Context, FrameLayout, OwnedImage},
    geometry::Rect,
    renderer::{DynamicTexture, Renderer},
};
use std::{collections::HashMap, os::fd::AsFd, rc::Rc};

pub(super) enum Outcome {
    Drawn,
    Waiting,
    Fallback,
}
pub(super) struct External {
    context: Rc<Context>,
    texture: DynamicTexture,
    images: HashMap<usize, OwnedImage>,
    generation: u64,
    failures: u32,
}
impl External {
    pub fn synchronize(&mut self, generation: u64) {
        if self.generation != generation {
            self.images.clear();
            self.generation = generation;
        }
    }
    pub fn new(context: Rc<Context>, renderer: &mut Renderer) -> Result<Self, Error> {
        renderer.camera_shader(Style::Cluster, true)?;
        let texture = renderer.dynamic_pixels(1, 1, &[0, 0, 0, 255])?;
        renderer.smooth_camera_plane(texture.id())?;
        Ok(Self {
            context,
            texture,
            images: HashMap::new(),
            generation: 0,
            failures: 0,
        })
    }
    pub fn draw(
        &mut self,
        input: &Input,
        renderer: &mut Renderer,
        destination: Rect,
    ) -> Result<Outcome, Error> {
        self.synchronize(input.generation());
        let Some(frame) = input.frame() else {
            return Ok(Outcome::Waiting);
        };
        let metadata = frame.metadata;
        let integer = |value| {
            i32::try_from(value)
                .map_err(|_| Error::Contract("camera layout exceeds native dimensions"))
        };
        if !self.images.contains_key(&metadata.index) {
            let layout = FrameLayout {
                width: integer(metadata.width)?,
                height: integer(metadata.height)?,
                stride: integer(metadata.stride)?,
                uv_offset: integer(metadata.uv_offset)?,
            };
            let buffer = input.retained_buffer()?;
            match self.context.create_owned(layout, buffer.as_fd()) {
                Ok(image) => {
                    self.images.insert(metadata.index, image);
                    self.failures = 0;
                }
                Err(error) => {
                    eprintln!("Cluster camera EGL import: {error}");
                    self.failures = self.failures.saturating_add(1);
                    return Ok(if self.failures >= 3 {
                        Outcome::Fallback
                    } else {
                        Outcome::Waiting
                    });
                }
            }
        }
        let image = self
            .images
            .get(&metadata.index)
            .ok_or(Error::Contract("camera EGL image missing"))?;
        renderer.flush_camera();
        image.bind(renderer.native_texture(self.texture.id())?);
        renderer.camera(CameraDraw {
            luma: self.texture.id(),
            chroma: None,
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: metadata
                    .width
                    .to_f32()
                    .ok_or(Error::Contract("camera width exceeds float range"))?,
                height: metadata
                    .height
                    .to_f32()
                    .ok_or(Error::Contract("camera height exceeds float range"))?,
            },
            destination,
            style: Style::Cluster,
        })?;
        Ok(Outcome::Drawn)
    }
}
