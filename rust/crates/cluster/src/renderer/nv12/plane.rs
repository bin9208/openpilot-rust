use super::{
    super::{graphics::scalar, Result},
    Nv12Layout,
};
use openpilot_startup_ui::draw::Draw;
use openpilot_startup_ui::{
    geometry::{Point, Rect},
    renderer::{RenderTarget, Renderer, ShaderResource, TargetPaint},
};

#[derive(Clone, Copy)]
pub(super) struct PlaneDraw<'a> {
    shader: &'a ShaderResource,
    source: &'a RenderTarget,
    layout: Nv12Layout,
    flip_x: bool,
}
#[derive(Clone, Copy)]
pub(super) struct TargetDraw<'a> {
    source: PlaneDraw<'a>,
    target: Option<&'a RenderTarget>,
    size: (i32, i32),
    plane: i32,
    y: i32,
    clear: Option<[u8; 4]>,
}
impl<'a> PlaneDraw<'a> {
    pub const fn new(
        shader: &'a ShaderResource,
        source: &'a RenderTarget,
        layout: Nv12Layout,
        flip_x: bool,
    ) -> Self {
        Self {
            shader,
            source,
            layout,
            flip_x,
        }
    }
    pub const fn with_target(
        self,
        target: &'a RenderTarget,
        size: (i32, i32),
        plane: i32,
        y: i32,
        clear: Option<[u8; 4]>,
    ) -> TargetDraw<'a> {
        TargetDraw {
            source: self,
            target: Some(target),
            size,
            plane,
            y,
            clear,
        }
    }
    pub const fn with_current(
        self,
        size: (i32, i32),
        plane: i32,
        y: i32,
        clear: Option<[u8; 4]>,
    ) -> TargetDraw<'a> {
        TargetDraw {
            source: self,
            target: None,
            size,
            plane,
            y,
            clear,
        }
    }
}
pub(super) fn draw(backend: &mut Renderer, draw: TargetDraw<'_>) -> Result<()> {
    let source = draw.source;
    let width = scalar(f64::from(source.layout.width))?;
    let height = scalar(f64::from(source.layout.height))?;
    let packed_width = scalar(f64::from(draw.size.0))?;
    let packed_height = scalar(f64::from(draw.size.1))?;
    backend.shader_vec2(source.shader, "srcSize", [width, height])?;
    backend.shader_vec2(source.shader, "packedSize", [packed_width, packed_height])?;
    backend.shader_integer(source.shader, "plane", draw.plane)?;
    backend.shader_integer(source.shader, "flipX", i32::from(source.flip_x))?;
    if let Some(target) = draw.target {
        backend.begin_target(target)?;
    }
    let result: Result<()> = (|| {
        if let Some(clear) = draw.clear {
            backend.clear(u32::from_le_bytes(clear))?;
        }
        backend.begin_shader(source.shader)?;
        backend.custom_blend(1, 0, 0x8006);
        let result = backend.target_texture(
            source.source,
            TargetPaint {
                source: Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                },
                destination: Rect {
                    x: 0.0,
                    y: scalar(f64::from(draw.y))?,
                    width: packed_width,
                    height: packed_height,
                },
                origin: Point { x: 0.0, y: 0.0 },
                rotation: 0.0,
                tint: u32::MAX,
            },
        );
        backend.end_blend();
        backend.end_shader();
        result?;
        Ok(())
    })();
    if draw.target.is_some() {
        backend.end_target();
    }
    result
}
