use super::{input::Input, Error};
use num_traits::ToPrimitive;
use openpilot_startup_ui::{
    camera::{CameraDraw, PlaneFormat, Style},
    geometry::Rect,
    renderer::{DynamicTexture, Renderer},
};

#[derive(Default)]
pub(super) struct Textures {
    luma: Option<DynamicTexture>,
    chroma: Option<DynamicTexture>,
    shape: Option<(usize, usize)>,
    revision: Option<u64>,
    generation: u64,
}
impl Textures {
    pub fn clear(&mut self) {
        self.luma = None;
        self.chroma = None;
        self.shape = None;
        self.revision = None;
    }
    pub fn draw(
        &mut self,
        input: &Input,
        renderer: &mut Renderer,
        destination: Rect,
    ) -> Result<bool, Error> {
        if self.generation != input.generation() {
            self.clear();
            self.generation = input.generation();
        }
        let Some(frame) = input.frame() else {
            return Ok(false);
        };
        let metadata = frame.metadata;
        let shape = (metadata.stride, metadata.height);
        if self.shape != Some(shape) {
            self.clear();
            let integer = |value| {
                i32::try_from(value)
                    .map_err(|_| Error::Contract("camera dimensions exceed native range"))
            };
            let luma = renderer.camera_plane(
                (integer(metadata.stride)?, integer(metadata.height)?),
                PlaneFormat::Luma,
            )?;
            renderer.smooth_camera_plane(luma.id())?;
            self.luma = Some(luma);
            let chroma = renderer.camera_plane(
                (integer(metadata.stride / 2)?, integer(metadata.height / 2)?),
                PlaneFormat::Chroma,
            )?;
            renderer.smooth_camera_plane(chroma.id())?;
            self.chroma = Some(chroma);
            self.shape = Some(shape);
        }
        let luma = self
            .luma
            .as_ref()
            .ok_or(Error::Contract("camera luma missing"))?;
        let chroma = self
            .chroma
            .as_ref()
            .ok_or(Error::Contract("camera chroma missing"))?;
        if self.revision != Some(input.revision()) {
            let (y, uv) = frame.bytes.split_at(metadata.uv_offset);
            renderer.update_camera_plane(luma.id(), y)?;
            renderer.update_camera_plane(chroma.id(), uv)?;
            self.revision = Some(input.revision());
        }
        renderer.camera(CameraDraw {
            luma: luma.id(),
            chroma: Some(chroma.id()),
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
        Ok(true)
    }
}
