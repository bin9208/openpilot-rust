use openpilot_ui_framework::{
    canvas::Canvas,
    draw::{Draw, ImageDraw, PolygonPaint, RoundedOutline, TextDraw},
    geometry::{Point, Rect},
    text::{Font, Measure},
    Error,
};
use serde_json::{json, Value};
use std::cell::RefCell;
pub struct Recording<'a> {
    pub canvas: &'a mut Canvas,
    pub commands: RefCell<Vec<Value>>,
}
fn xy(p: Point) -> [f32; 2] {
    [p.x, p.y]
}
fn rect(r: Rect) -> [f32; 4] {
    [r.x, r.y, r.width, r.height]
}
impl Recording<'_> {
    fn push(&self, value: Value) {
        self.commands.borrow_mut().push(value);
    }
}
impl Measure for Recording<'_> {
    fn measure(&self, font: Font, text: &str, size: f32, spacing: f32) -> Point {
        self.canvas.measure(font, text, size, spacing)
    }
    fn measure_default(&self, text: &str, size: i32) -> Result<i32, Error> {
        let value = self.canvas.measure_default(text, size)?;
        self.push(json!({"kind":"measure_default","text":text,"size":size,"value":value}));
        Ok(value)
    }
}
impl Draw for Recording<'_> {
    fn font_scale(&self) -> f64 {
        self.canvas.font_scale()
    }
    fn text(&mut self, t: TextDraw<'_>) -> Result<(), Error> {
        self.push(json!({"kind":"text","text":t.text,"position":xy(t.position),"size":t.size,"spacing":t.spacing,"color":t.color}));
        self.canvas.text(t)
    }
    fn triangle_strip(&mut self, p: &[Point], c: u32) -> Result<(), Error> {
        self.push(json!({"kind":"strip","points":p.iter().copied().map(xy).collect::<Vec<_>>(),"color":c}));
        self.canvas.triangle_strip(p, c)
    }
    fn shaded_strip(&mut self, p: &[Point], paint: PolygonPaint<'_>) -> Result<(), Error> {
        let value = match &paint {
            PolygonPaint::Color(c) => json!({"color":c}),
            PolygonPaint::Gradient {
                start,
                end,
                colors,
                stops,
            } => json!({"start":xy(*start),"end":xy(*end),"colors":colors,"stops":stops}),
        };
        self.push(json!({"kind":"shader","points":p.iter().copied().map(xy).collect::<Vec<_>>(),"paint":value}));
        self.canvas.shaded_strip(p, paint)
    }
    fn line(&mut self, a: Point, b: Point, t: f32, c: u32) -> Result<(), Error> {
        self.push(json!({"kind":"line","start":xy(a),"end":xy(b),"thickness":t,"color":c}));
        self.canvas.line(a, b, t, c)
    }
    fn circle(&mut self, p: Point, r: f32, c: u32) -> Result<(), Error> {
        self.push(json!({"kind":"circle","position":xy(p),"radius":r,"color":c}));
        self.canvas.circle(p, r, c)
    }
    fn rounded_segments(
        &mut self,
        r: Rect,
        roundness: f32,
        segments: i32,
        c: u32,
        border: bool,
    ) -> Result<(), Error> {
        self.push(json!({"kind":"rounded","rect":rect(r),"roundness":roundness,"segments":segments,"color":c,"border":border}));
        self.canvas
            .rounded_segments(r, roundness, segments, c, border)
    }
    fn rounded_outline(&mut self, r: Rect, style: RoundedOutline) -> Result<(), Error> {
        self.push(json!({"kind":"rounded_outline","rect":rect(r),"roundness":style.roundness,"segments":style.segments,"thickness":style.thickness,"color":style.color}));
        self.canvas.rounded_outline(r, style)
    }
    fn rounded(&mut self, r: Rect, k: f32, c: u32) -> Result<(), Error> {
        self.canvas.rounded(r, k, c)
    }
    fn border(&mut self, r: Rect, k: f32, c: u32) -> Result<(), Error> {
        self.canvas.border(r, k, c)
    }
    fn texture(&mut self, track: bool, r: Rect, o: Point, t: f32) -> Result<(), Error> {
        Draw::texture(self.canvas, track, r, o, t)
    }
    fn scissor(&mut self, r: Option<Rect>) -> Result<(), Error> {
        self.canvas.scissor(r)
    }
    fn image(&mut self, image: ImageDraw) -> Result<(), Error> {
        self.canvas.image(image)
    }
}
