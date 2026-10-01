//! Safe frame, input and diagnostic controls for the owning raylib surface.
use crate::{bridge::ffi, geometry::Point, renderer::Renderer, Error};
use std::collections::{BTreeSet, VecDeque};
#[derive(Default)]
pub struct KeyboardSnapshot {
    pub queued: VecDeque<i32>,
    pub characters: VecDeque<u32>,
    pub down: BTreeSet<i32>,
    pub pressed: BTreeSet<i32>,
}
impl Renderer {
    pub fn screen_screenshot(&self, path: &std::path::Path) -> Result<(), Error> {
        Ok(self.surface.screen_screenshot(
            path.to_str()
                .ok_or(Error::Contract("screenshot path is not UTF-8"))?,
        )?)
    }
    pub fn rectangle_lines(
        &mut self,
        rect: crate::geometry::Rect,
        color: u32,
    ) -> Result<(), Error> {
        use crate::number::coordinate;
        self.surface.pin_mut().rectangle_lines(
            coordinate(rect.x)?,
            coordinate(rect.y)?,
            coordinate(rect.width)?,
            coordinate(rect.height)?,
            color,
        );
        Ok(())
    }

    pub fn dimensions(&self) -> (f32, f32) {
        self.dimensions
    }
    pub fn set_title(&mut self, title: &str) {
        self.surface.pin_mut().set_title(title);
    }
    pub fn set_target_fps(&mut self, fps: i32) {
        self.surface.pin_mut().target_fps(fps);
    }
    pub fn fps(&self) -> i32 {
        self.surface.fps()
    }
    pub fn draw_fps(&mut self, position: (i32, i32)) {
        self.surface.pin_mut().draw_fps(position.0, position.1);
    }
    pub fn ensure_render_target(&mut self, dimensions: (i32, i32)) -> Result<(), Error> {
        Ok(self
            .surface
            .pin_mut()
            .render_target(dimensions.0, dimensions.1)?)
    }
    pub fn has_render_target(&self) -> bool {
        self.surface.has_render_target()
    }
    pub fn capture_pixels(&self) -> Result<Vec<u8>, Error> {
        Ok(self.surface.capture_pixels()?)
    }
    pub fn finish_content(&mut self) {
        self.surface.pin_mut().finish_content(self.config.scale);
    }
    pub fn present(&mut self) {
        self.surface.pin_mut().present();
    }
    pub fn burn_in(&mut self) -> Result<(), Error> {
        let version = if cfg!(target_os = "macos") {
            "#version 330 core\n"
        } else {
            "#version 300 es\nprecision highp float;\n"
        };
        let vertex=format!("{version}in vec3 vertexPosition;in vec2 vertexTexCoord;uniform mat4 mvp;out vec2 fragTexCoord;void main(){{fragTexCoord=vertexTexCoord;gl_Position=mvp*vec4(vertexPosition,1.0);}}");
        let fragment=format!("{version}in vec2 fragTexCoord;uniform sampler2D texture0;out vec4 fragColor;void main(){{vec4 sampled=texture(texture0,fragTexCoord);float intensity=sampled.b;vec3 start=vec3(0.0,1.0,0.0);vec3 middle=vec3(1.0,1.0,0.0);vec3 end=vec3(1.0,0.0,0.0);vec3 gradient=mix(start,middle,clamp(intensity*2.0,0.0,1.0));gradient=mix(gradient,end,clamp((intensity-0.5)*2.0,0.0,1.0));fragColor=vec4(gradient,sampled.a);}}");
        Ok(self.surface.pin_mut().burn_in(&vertex, &fragment)?)
    }
    pub fn poll_input(&self) {
        ffi::poll_input();
    }
    pub fn mouse_position(&self) -> Point {
        let p = self.surface.mouse_position();
        Point {
            x: p.x / self.config.scale,
            y: p.y / self.config.scale,
        }
    }
    pub fn keyboard(&self) -> KeyboardSnapshot {
        let mut input = KeyboardSnapshot::default();
        loop {
            let key = self.surface.key_pressed();
            if key == 0 {
                break;
            }
            input.queued.push_back(key);
        }
        loop {
            let character = self.surface.char_pressed();
            if character == 0 {
                break;
            }
            if let Ok(character) = u32::try_from(character) {
                input.characters.push_back(character);
            }
        }
        for key in 0..512 {
            if self.surface.key_down(key) {
                input.down.insert(key);
            }
            if self.surface.key_started(key) {
                input.pressed.insert(key);
            }
        }
        input
    }
}
