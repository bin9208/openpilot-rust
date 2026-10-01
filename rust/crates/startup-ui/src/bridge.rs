// SAFETY: CXX owns all opaque resources; the C++ adapter checks indices, copies
// borrowed strings/slices, never retains Rust pointers, and releases raylib objects once.
#[cxx::bridge(namespace = "startup_ui")]
pub mod ffi {
    extern "Rust" {
        fn trace_log(level: i32, message: &[u8]);
    }
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
        include!("egl.h");
        type EglApi;
        fn egl_api(egl_path: &str, gles_path: &str) -> Result<UniquePtr<EglApi>>;
        fn current_display(self: &EglApi) -> u64;
        fn initialize(self: &EglApi, display: u64) -> bool;
        fn extensions(self: &EglApi, display: u64) -> String;
        fn error(self: &EglApi) -> i32;
        fn create_image(self: &EglApi, display: u64, attributes: &[i32]) -> Result<u64>;
        fn destroy_image(self: &EglApi, display: u64, image: u64) -> bool;
        fn bind_image(self: &EglApi, texture: u32, image: u64);
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
        fn clear(self: Pin<&mut Surface>, color: u32);
        fn texture_release(self: Pin<&mut Surface>, texture: u32);
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
        fn circle_gradient(
            self: Pin<&mut Surface>,
            center: Point,
            radius: f32,
            inner: u32,
            outer: u32,
        );
        fn gradient(
            self: Pin<&mut Surface>,
            rect: Rect,
            top_left: u32,
            bottom_left: u32,
            top_right: u32,
            bottom_right: u32,
        );
        fn line(self: Pin<&mut Surface>, start: Point, end: Point, thick: f32, color: u32);
        fn rounded_segments(
            self: Pin<&mut Surface>,
            rect: Rect,
            roundness: f32,
            segments: i32,
            color: u32,
            border: bool,
        );
        fn measure_default(self: &Surface, text: &str, size: i32) -> Result<i32>;
        fn rounded_outline(
            self: Pin<&mut Surface>,
            rect: Rect,
            roundness: f32,
            segments: i32,
            thickness: f32,
            color: u32,
        );
        fn rounded(self: Pin<&mut Surface>, rect: Rect, roundness: f32, color: u32, border: bool);
        fn scissor(self: Pin<&mut Surface>, rect: Rect, enabled: bool);
        fn render_target(self: Pin<&mut Surface>, width: i32, height: i32) -> Result<()>;
        fn has_render_target(self: &Surface) -> bool;
        fn burn_in(self: Pin<&mut Surface>, vertex: &str, fragment: &str) -> Result<()>;
        fn capture_pixels(self: &Surface) -> Result<Vec<u8>>;
        fn shader_load(self: Pin<&mut Surface>, vertex: &str, fragment: &str) -> Result<u32>;
        fn shader_unload(self: Pin<&mut Surface>, shader: u32) -> Result<()>;
        fn uniform_floats(
            self: Pin<&mut Surface>,
            shader: u32,
            name: &str,
            values: &[f32],
            kind: i32,
            count: i32,
        ) -> Result<()>;
        fn uniform_int(self: Pin<&mut Surface>, shader: u32, name: &str, value: i32) -> Result<()>;
        fn uniform_matrix(
            self: Pin<&mut Surface>,
            shader: u32,
            name: &str,
            values: &[f32],
        ) -> Result<()>;
        fn triangle_strip(
            self: Pin<&mut Surface>,
            points: &[Point],
            color: u32,
            shader: u32,
            shaded: bool,
        ) -> Result<()>;
        fn set_title(self: Pin<&mut Surface>, title: &str);
        fn fps(self: &Surface) -> i32;
        fn draw_fps(self: Pin<&mut Surface>, x: i32, y: i32);
        fn key_pressed(self: &Surface) -> i32;
        fn char_pressed(self: &Surface) -> i32;
        fn key_down(self: &Surface, key: i32) -> bool;
        fn key_started(self: &Surface, key: i32) -> bool;
        fn mouse_position(self: &Surface) -> Point;
        fn finish_content(self: Pin<&mut Surface>, scale: f32);
        fn present(self: Pin<&mut Surface>);
        fn begin(self: Pin<&mut Surface>, scale: f32);
        fn end(self: Pin<&mut Surface>, scale: f32);
        fn screenshot(self: &Surface, path: &str) -> Result<()>;
        fn screen_screenshot(self: &Surface, path: &str) -> Result<()>;
        fn rectangle_lines(
            self: Pin<&mut Surface>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            color: u32,
        );
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

fn trace_log(level: i32, message: &[u8]) {
    crate::logging::trace_log(level, &String::from_utf8_lossy(message));
}
