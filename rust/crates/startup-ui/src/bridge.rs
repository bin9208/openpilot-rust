// SAFETY: CXX owns all opaque resources; the C++ adapter checks indices, copies
// borrowed strings/slices, never retains Rust pointers, and releases raylib objects once.
#[cxx::bridge(namespace = "startup_ui")]
pub mod ffi {
    extern "Rust" {
        fn trace_log(level: i32, message: &[u8]);
    }
    #[derive(Clone, Copy, Debug, Default)]
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
    #[derive(Clone, Copy, Debug)]
    struct Ring {
        center: Point,
        inner: f32,
        outer: f32,
        start: f32,
        end: f32,
        segments: i32,
        color: u32,
    }
    #[derive(Clone, Copy, Debug)]
    struct TextPaint {
        font: u32,
        position: Point,
        size: f32,
        spacing: f32,
        color: u32,
    }
    #[derive(Clone, Copy, Debug)]
    struct Vector3 {
        x: f32,
        y: f32,
        z: f32,
    }
    #[derive(Clone, Copy, Debug)]
    struct Camera3D {
        position: Vector3,
        target: Vector3,
        up: Vector3,
        fovy: f32,
        projection: i32,
    }
    #[derive(Clone, Copy, Debug)]
    struct TargetInfo {
        framebuffer: u32,
        texture: u32,
        width: i32,
        height: i32,
    }
    #[derive(Clone, Copy, Debug)]
    struct TexturePaint {
        source: Rect,
        destination: Rect,
        origin: Point,
        rotation: f32,
        tint: u32,
    }
    #[derive(Clone, Copy, Debug)]
    struct ModelPaint {
        position: Vector3,
        rotation_axis: Vector3,
        rotation_angle: f32,
        scale: Vector3,
        tint: u32,
    }
    unsafe extern "C++" {
        include!("bridge.h");
        include!("egl.h");
        include!("gles.h");
        type GlesApi;
        type GlMapping;
        fn gles_api(path: &str) -> Result<UniquePtr<GlesApi>>;
        fn integer(self: &GlesApi, parameter: u32) -> i32;
        fn gl_error(self: &GlesApi) -> u32;
        fn bind_framebuffer(self: &GlesApi, target: u32, name: u32);
        fn bind_buffer(self: &GlesApi, target: u32, name: u32);
        fn pixel_store(self: &GlesApi, parameter: u32, value: i32);
        fn read_rgba(self: &GlesApi, width: i32, height: i32, destination: &mut [u8])
            -> Result<()>;
        fn read_rgba_pbo(self: &GlesApi, width: i32, height: i32) -> Result<()>;
        fn async_available(self: &GlesApi) -> bool;
        fn prepare_dma(self: &GlesApi) -> Result<()>;
        fn generate_buffers(self: &GlesApi, count: u32) -> Result<Vec<u32>>;
        fn delete_buffers(self: &GlesApi, names: &[u32]) -> Result<()>;
        fn allocate_buffer(self: &GlesApi, target: u32, size: u64, usage: u32) -> Result<()>;
        fn fence(self: &GlesApi, condition: u32, flags: u32) -> Result<u64>;
        fn wait_fence(self: &GlesApi, fence: u64, flags: u32, timeout: u64) -> Result<u32>;
        fn delete_fence(self: &GlesApi, fence: u64) -> Result<()>;
        fn flush(self: &GlesApi) -> Result<()>;
        fn map_read(self: &GlesApi, target: u32, size: u64) -> Result<UniquePtr<GlMapping>>;
        fn copy_to(self: &GlMapping, destination: &mut [u8]) -> Result<()>;
        fn unmap(self: Pin<&mut GlMapping>) -> bool;
        fn generate_texture(self: &GlesApi) -> Result<u32>;
        fn delete_texture(self: &GlesApi, texture: u32) -> Result<()>;
        fn bind_texture(self: &GlesApi, target: u32, texture: u32) -> Result<()>;
        fn texture_parameter(self: &GlesApi, target: u32, parameter: u32, value: i32)
            -> Result<()>;
        fn generate_framebuffer(self: &GlesApi) -> Result<u32>;
        fn delete_framebuffer(self: &GlesApi, framebuffer: u32) -> Result<()>;
        fn attach_texture(self: &GlesApi, target: u32, attachment: u32, texture: u32)
            -> Result<()>;
        fn framebuffer_status(self: &GlesApi, target: u32) -> Result<u32>;
        fn try_process_locale(candidate: &str) -> Result<bool>;
        type EglApi;
        fn egl_api(egl_path: &str, gles_path: &str) -> Result<UniquePtr<EglApi>>;
        fn egl_image_api(egl_path: &str, gles_path: &str) -> Result<UniquePtr<EglApi>>;
        fn prepare_images(self: &EglApi) -> bool;
        fn image_target(self: &EglApi, target: u32, image: u64) -> Result<()>;
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
        fn create_with_trace(
            width: i32,
            height: i32,
            title: &str,
            flags: u32,
            level: i32,
        ) -> Result<UniquePtr<Surface>>;
        fn create_inheriting_flags(
            width: i32,
            height: i32,
            title: &str,
            level: i32,
        ) -> Result<UniquePtr<Surface>>;
        fn trace_level(level: i32);
        fn monitor() -> Result<Point>;
        fn image(path: &str) -> Result<UniquePtr<Image>>;
        fn image_encoded(extension: &str, bytes: &[u8]) -> Result<UniquePtr<Image>>;
        fn image_blank(width: i32, height: i32, color: u32) -> Result<UniquePtr<Image>>;
        fn export_rgba(width: i32, height: i32, bytes: &[u8], path: &str) -> Result<()>;
        fn width(self: &Image) -> i32;
        fn height(self: &Image) -> i32;
        fn rgba(self: Pin<&mut Image>) -> Result<Vec<u8>>;
        fn rgba_view(self: &Image) -> Result<&[u8]>;
        fn format(self: &Image) -> i32;
        fn rgba_format(self: Pin<&mut Image>);
        fn flip_vertical(self: Pin<&mut Image>);
        fn encode(self: &Image, extension: &str) -> Result<Vec<u8>>;
        fn export_file(self: &Image, path: &str) -> Result<()>;
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
        fn default_font(self: Pin<&mut Surface>) -> Result<u32>;
        fn font_bilinear(self: Pin<&mut Surface>, font: u32) -> Result<()>;
        fn rectangle_rec(self: Pin<&mut Surface>, rect: Rect, color: u32);
        fn rectangle_lines_ex(self: Pin<&mut Surface>, rect: Rect, thickness: f32, color: u32);
        fn triangle(self: Pin<&mut Surface>, a: Point, b: Point, c: Point, color: u32);
        fn push_matrix(self: Pin<&mut Surface>);
        fn pop_matrix(self: Pin<&mut Surface>);
        fn scale(self: Pin<&mut Surface>, x: f32, y: f32, z: f32);
        fn translate(self: Pin<&mut Surface>, x: f32, y: f32, z: f32);
        fn begin_capture(self: Pin<&mut Surface>) -> Result<()>;
        fn end_capture(self: Pin<&mut Surface>);
        fn capture_info(self: &Surface) -> Result<TargetInfo>;
        fn capture_draw(self: Pin<&mut Surface>, paint: TexturePaint) -> Result<()>;
        fn capture_filter(self: Pin<&mut Surface>, filter: i32) -> Result<()>;
        fn capture_image(self: &Surface) -> Result<UniquePtr<Image>>;
        fn target_image(self: &Surface, index: u32) -> Result<UniquePtr<Image>>;
        fn text_batch(self: Pin<&mut Surface>, text: &str, paints: &[TextPaint]) -> Result<()>;
        fn frame_begin(self: Pin<&mut Surface>);
        fn frame_end(self: Pin<&mut Surface>);
        fn image_text(
            self: Pin<&mut Surface>,
            image: Pin<&mut Image>,
            text: &str,
            paint: TextPaint,
        ) -> Result<()>;
        fn texture_filter(self: Pin<&mut Surface>, texture: u32, filter: i32) -> Result<()>;
        fn texture_size(
            self: Pin<&mut Surface>,
            texture: u32,
            width: i32,
            height: i32,
        ) -> Result<()>;
        fn uniform_texture(
            self: Pin<&mut Surface>,
            shader: u32,
            name: &str,
            texture: u32,
        ) -> Result<()>;
        type Points3D;
        fn points3d(values: &[f32]) -> Result<UniquePtr<Points3D>>;
        type Points2D;
        fn points2d(capacity: u32) -> Result<UniquePtr<Points2D>>;
        fn update(self: Pin<&mut Points2D>, values: &[Point]) -> Result<()>;
        fn strip2d(
            self: Pin<&mut Surface>,
            points: &Points2D,
            offset: u32,
            count: u32,
            color: u32,
        ) -> Result<()>;
        fn strip2d_available(self: &Surface) -> bool;
        fn mode3d_begin(self: Pin<&mut Surface>, camera: Camera3D);
        fn mode3d_end(self: Pin<&mut Surface>);
        fn viewport(self: Pin<&mut Surface>, x: i32, y: i32, width: i32, height: i32);
        fn triangle3d(self: Pin<&mut Surface>, a: Vector3, b: Vector3, c: Vector3, color: u32);
        fn line3d(self: Pin<&mut Surface>, a: Vector3, b: Vector3, color: u32);
        fn sphere(
            self: Pin<&mut Surface>,
            center: Vector3,
            radius: f32,
            rings: i32,
            slices: i32,
            color: u32,
        );
        fn cube_v(self: Pin<&mut Surface>, center: Vector3, size: Vector3, color: u32);
        fn strip3d(self: Pin<&mut Surface>, points: &Points3D, color: u32) -> Result<()>;
        fn strip_available(self: &Surface) -> bool;
        fn world_to_screen(
            self: &Surface,
            point: Vector3,
            camera: Camera3D,
            width: i32,
            height: i32,
        ) -> Point;
        fn culling(self: Pin<&mut Surface>, enabled: bool);
        fn vertices_begin(self: Pin<&mut Surface>, mode: i32);
        fn vertex3(self: Pin<&mut Surface>, point: Vector3);
        fn color4(self: Pin<&mut Surface>, color: u32);
        fn vertices_end(self: Pin<&mut Surface>);
        fn target_create(self: Pin<&mut Surface>, width: i32, height: i32) -> Result<u32>;
        fn target_release(self: Pin<&mut Surface>, target: u32) -> Result<()>;
        fn target_begin(self: Pin<&mut Surface>, target: u32) -> Result<()>;
        fn external_target_begin(self: Pin<&mut Surface>, target: TargetInfo) -> Result<()>;
        fn target_end(self: Pin<&mut Surface>);
        fn target_info(self: &Surface, target: u32) -> Result<TargetInfo>;
        fn target_pixels(self: &Surface, target: u32, flip_vertical: bool) -> Result<Vec<u8>>;
        fn target_draw(self: Pin<&mut Surface>, target: u32, paint: TexturePaint) -> Result<()>;
        fn target_filter(self: Pin<&mut Surface>, target: u32, filter: i32) -> Result<()>;
        fn mesh_create(
            self: Pin<&mut Surface>,
            vertices: &[f32],
            normals: &[f32],
            colors: &[u8],
        ) -> Result<u32>;
        fn mesh_upload(self: Pin<&mut Surface>, mesh: u32, dynamic: bool) -> Result<()>;
        fn mesh_release(self: Pin<&mut Surface>, mesh: u32) -> Result<()>;
        fn model_from_mesh(self: Pin<&mut Surface>, mesh: u32) -> Result<u32>;
        fn model_release(self: Pin<&mut Surface>, model: u32) -> Result<()>;
        fn model_draw(self: Pin<&mut Surface>, model: u32, paint: ModelPaint) -> Result<()>;
        fn shader_begin(self: Pin<&mut Surface>, shader: u32) -> Result<()>;
        fn shader_end(self: Pin<&mut Surface>);
        fn blend_custom(self: Pin<&mut Surface>, source: i32, destination: i32, equation: i32);
        fn blend_end(self: Pin<&mut Surface>);
        fn texture_release(self: Pin<&mut Surface>, texture: u32);
        fn pixel_texture(
            self: Pin<&mut Surface>,
            width: i32,
            height: i32,
            rgba: &[u8],
        ) -> Result<u32>;
        fn smooth_texture(self: Pin<&mut Surface>, texture: u32) -> Result<()>;
        fn pixel_update(self: Pin<&mut Surface>, texture: u32, rgba: &[u8]) -> Result<()>;
        fn ring(self: Pin<&mut Surface>, ring: Ring);
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
        fn circle_lines(self: Pin<&mut Surface>, x: i32, y: i32, radius: f32, color: u32);
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
        fn spline(self: Pin<&mut Surface>, points: &[Point], thick: f32, color: u32) -> Result<()>;
        fn line(self: Pin<&mut Surface>, start: Point, end: Point, thick: f32, color: u32);
        fn integer_line(self: Pin<&mut Surface>, x1: i32, y1: i32, x2: i32, y2: i32, color: u32);
        fn default_text(
            self: Pin<&mut Surface>,
            text: &str,
            x: i32,
            y: i32,
            size: i32,
            color: u32,
        ) -> Result<()>;
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
