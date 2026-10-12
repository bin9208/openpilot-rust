use super::{cache::CachedTarget, plane::PlaneDraw, shaders, Nv12Layout};
use crate::renderer::{graphics::scalar, Error, Result};
use openpilot_startup_ui::{
    draw::Draw,
    geometry::{Point, Rect},
    renderer::{Renderer, ShaderResource, TargetPaint},
};

pub(super) fn prepare<'a>(
    backend: &mut Renderer,
    upload: &'a mut CachedTarget,
    shader: &'a mut Option<ShaderResource>,
    layout: Nv12Layout,
    flip_x: bool,
    background: u32,
) -> Result<PlaneDraw<'a>> {
    let capture = backend.capture_info()?;
    let upload = upload.get(backend, (layout.width, layout.height))?;
    backend.begin_target(upload)?;
    let transform: Result<()> = (|| {
        backend.clear(background)?;
        backend.capture_texture(TargetPaint {
            source: Rect {
                x: 0.0,
                y: 0.0,
                width: scalar(f64::from(capture.width))?,
                height: scalar(f64::from(capture.height))?,
            },
            destination: Rect {
                x: 0.0,
                y: scalar(f64::from(capture.width))?,
                width: scalar(f64::from(capture.width))?,
                height: scalar(f64::from(capture.height))?,
            },
            origin: Point { x: 0.0, y: 0.0 },
            rotation: -90.0,
            tint: u32::MAX,
        })?;
        Ok(())
    })();
    backend.end_target();
    transform?;
    if shader.is_none() {
        *shader = Some(backend.load_shader(shaders::VERTEX, shaders::FRAGMENT)?);
    }
    let shader = shader
        .as_ref()
        .ok_or(Error::Contract("NV12 pack shader missing"))?;
    Ok(PlaneDraw::new(shader, upload, layout, flip_x))
}
