use crate::{bridge::ffi, draw::PolygonPaint, geometry::Point, number, renderer::Renderer, Error};
const VERTEX:&str="in vec3 vertexPosition;in vec2 vertexTexCoord;out vec2 fragTexCoord;uniform mat4 mvp;void main(){fragTexCoord=vertexTexCoord;gl_Position=mvp*vec4(vertexPosition,1.0);}";
const FRAGMENT: &str = r#"
in vec2 fragTexCoord;
out vec4 finalColor;
uniform vec4 fillColor;
uniform int useGradient;
uniform vec2 gradientStart;
uniform vec2 gradientEnd;
uniform vec4 gradientColors[20];
uniform float gradientStops[20];
uniform int gradientColorCount;
vec4 getGradientColor(vec2 p) {
  vec2 d = gradientStart - gradientEnd;
  float len2 = max(dot(d, d), 1e-6);
  float t = clamp(dot(p - gradientEnd, d) / len2, 0.0, 1.0);
  float t0 = gradientStops[0];
  float tn = gradientStops[gradientColorCount-1];
  if (t <= t0) return gradientColors[0];
  if (t >= tn) return gradientColors[gradientColorCount-1];
  for (int i = 0; i < gradientColorCount - 1; i++) {
    float a = gradientStops[i];
    float b = gradientStops[i+1];
    if (t >= a && t <= b) {
      float k = (t - a) / max(b - a, 1e-6);
      return mix(gradientColors[i], gradientColors[i+1], k);
    }
  }
  return gradientColors[gradientColorCount-1];
}
void main() {
  finalColor = useGradient == 1 ? getGradientColor(gl_FragCoord.xy) : fillColor;
}
"#;
impl Renderer {
    pub fn cleanup_polygon(&mut self) -> Result<(), Error> {
        if let Some(id) = self.polygon_shader.take() {
            self.surface.pin_mut().shader_unload(id)?;
        }
        Ok(())
    }
    pub fn triangle_strip(&mut self, points: &[Point], color: u32) -> Result<(), Error> {
        let points: Vec<_> = points
            .iter()
            .map(|p| ffi::Point { x: p.x, y: p.y })
            .collect();
        Ok(self
            .surface
            .pin_mut()
            .triangle_strip(&points, color, 0, false)?)
    }
    pub fn shaded_strip(&mut self, points: &[Point], paint: PolygonPaint<'_>) -> Result<(), Error> {
        let id = if let Some(id) = self.polygon_shader {
            id
        } else {
            let version = if cfg!(target_os = "macos") {
                "#version 330 core\n"
            } else {
                "#version 300 es\nprecision highp float;\n"
            };
            let id = self.surface.pin_mut().shader_load(
                &format!("{version}{VERTEX}"),
                &format!("{version}{FRAGMENT}"),
            )?;
            let (width, height) = self.dimensions();
            self.surface.pin_mut().uniform_matrix(
                id,
                "mvp",
                &[
                    number::float(2.0 / f64::from(width)),
                    0.0,
                    0.0,
                    -1.0,
                    0.0,
                    number::float(-2.0 / f64::from(height)),
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                    -1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ],
            )?;
            self.polygon_shader = Some(id);
            id
        };
        match paint {
            PolygonPaint::Color(color) => {
                self.surface.pin_mut().uniform_int(id, "useGradient", 0)?;
                let color = color
                    .to_le_bytes()
                    .map(|value| number::float(f64::from(value) / 255.0));
                self.surface
                    .pin_mut()
                    .uniform_floats(id, "fillColor", &color, 3, 1)?;
            }
            PolygonPaint::Gradient {
                start,
                end,
                colors,
                stops,
            } => {
                if colors.is_empty() {
                    self.surface.pin_mut().uniform_int(id, "useGradient", 0)?;
                    self.surface
                        .pin_mut()
                        .uniform_floats(id, "fillColor", &[1.0; 4], 3, 1)?;
                } else {
                    if colors.len() > 20 || stops.len() > 20 {
                        return Err(Error::Contract("gradient exceeds 20 uniforms"));
                    }
                    let count = i32::try_from(colors.len())
                        .map_err(|_| Error::Contract("gradient count overflow"))?;
                    let colors: Vec<_> = colors
                        .iter()
                        .flat_map(|color| {
                            color
                                .to_le_bytes()
                                .map(|value| number::float(f64::from(value) / 255.0))
                        })
                        .collect();
                    self.surface.pin_mut().uniform_int(id, "useGradient", 1)?;
                    self.surface.pin_mut().uniform_floats(
                        id,
                        "gradientColors",
                        &colors,
                        3,
                        count,
                    )?;
                    self.surface.pin_mut().uniform_floats(
                        id,
                        "gradientStops",
                        stops,
                        0,
                        i32::try_from(stops.len())
                            .map_err(|_| Error::Contract("gradient stop count overflow"))?,
                    )?;
                    self.surface
                        .pin_mut()
                        .uniform_int(id, "gradientColorCount", count)?;
                    self.surface.pin_mut().uniform_floats(
                        id,
                        "gradientStart",
                        &[start.x, start.y],
                        1,
                        1,
                    )?;
                    self.surface.pin_mut().uniform_floats(
                        id,
                        "gradientEnd",
                        &[end.x, end.y],
                        1,
                        1,
                    )?;
                }
            }
        }
        let points: Vec<_> = points
            .iter()
            .map(|p| ffi::Point { x: p.x, y: p.y })
            .collect();
        Ok(self
            .surface
            .pin_mut()
            .triangle_strip(&points, u32::MAX, id, true)?)
    }
}
