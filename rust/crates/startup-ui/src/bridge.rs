// SAFETY: CXX owns all opaque resources; the C++ adapter checks indices, copies
// borrowed strings/slices, never retains Rust pointers, and releases raylib objects once.
#[cxx::bridge(namespace = "startup_ui")]
pub mod ffi {
    #[derive(Clone, Copy, Debug)]
    struct Point {
        x: f32,
        y: f32,
    }
    #[derive(Clone, Copy, Debug)]
    struct Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    }
    #[derive(Clone, Copy, Debug)]
    struct Sample {
        x: f32,
        y: f32,
        down: bool,
    }
    unsafe extern "C++" {
        include!("bridge.h");
        type Surface;
        type Image;
        fn create(width: i32, height: i32, title: &str, flags: u32) -> Result<UniquePtr<Surface>>;
        fn monitor() -> Result<Point>;
        fn image(path: &str) -> Result<UniquePtr<Image>>;
        fn width(self: &Image) -> i32;
        fn height(self: &Image) -> i32;
        fn premultiply(self: Pin<&mut Image>);
        fn flip_horizontal(self: Pin<&mut Image>);
        fn resize(self: Pin<&mut Image>, width: i32, height: i32);
        fn texture(
            self: Pin<&mut Surface>,
            image: Pin<&mut Image>,
            logical_width: i32,
            logical_height: i32,
        ) -> Result<u32>;
        fn pixel_texture(
            self: Pin<&mut Surface>,
            width: i32,
            height: i32,
            rgba: &[u8],
        ) -> Result<u32>;
        fn font(
            self: Pin<&mut Surface>,
            path: &str,
            size: i32,
            points: &[i32],
            atlas: bool,
            mipmaps: bool,
        ) -> Result<u32>;
        fn measure(self: &Surface, font: u32, text: &str, size: f32, spacing: f32)
            -> Result<Point>;
        fn text(
            self: Pin<&mut Surface>,
            font: u32,
            text: &str,
            position: Point,
            size: f32,
            spacing: f32,
            color: u32,
        ) -> Result<()>;
        fn draw_texture(
            self: Pin<&mut Surface>,
            texture: u32,
            rect: Rect,
            origin: Point,
            rotation: f32,
        ) -> Result<()>;
        fn tinted_texture(
            self: Pin<&mut Surface>,
            texture: u32,
            source: Rect,
            destination: Rect,
            origin: Point,
            rotation: f32,
            tint: u32,
        ) -> Result<()>;
        fn circle(self: Pin<&mut Surface>, center: Point, radius: f32, color: u32);
        fn gradient(
            self: Pin<&mut Surface>,
            rect: Rect,
            top_left: u32,
            bottom_left: u32,
            top_right: u32,
            bottom_right: u32,
        );
        fn line(self: Pin<&mut Surface>, start: Point, end: Point, thick: f32, color: u32);
        fn rounded(self: Pin<&mut Surface>, rect: Rect, roundness: f32, color: u32, border: bool);
        fn scissor(self: Pin<&mut Surface>, rect: Rect, enabled: bool);
        fn render_target(self: Pin<&mut Surface>, width: i32, height: i32) -> Result<()>;
        fn begin(self: Pin<&mut Surface>, scale: f32);
        fn end(self: Pin<&mut Surface>, scale: f32);
        fn screenshot(self: &Surface, path: &str) -> Result<()>;
        fn should_close(self: &Surface) -> bool;
        fn target_fps(self: Pin<&mut Surface>, fps: i32);
        fn frame_time(self: &Surface) -> f32;
        fn time(self: &Surface) -> f64;
        fn sample(self: &Surface, slot: i32) -> Sample;
        fn wheel(self: &Surface) -> f32;
        fn poll_input();
        fn sample_input(slot: i32) -> Sample;
    }
}
