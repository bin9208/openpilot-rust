#[cxx::bridge(namespace = "startup_ui")]
pub(crate) mod ffi {
    struct CameraRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    }

    // SAFETY: Surface owns every indexed texture/shader; C++ checks plane sizes and
    // consumes borrowed byte slices before returning without retaining pointers.
    unsafe extern "C++" {
        include!("bridge.h");
        type Surface = crate::bridge::ffi::Surface;
        fn plane_texture(
            self: Pin<&mut Surface>,
            width: i32,
            height: i32,
            chroma: bool,
        ) -> Result<u32>;
        fn plane_update(self: Pin<&mut Surface>, texture: u32, bytes: &[u8]) -> Result<()>;
        fn texture_native(self: &Surface, texture: u32) -> Result<u32>;
        fn camera_texture(
            self: Pin<&mut Surface>,
            shader: u32,
            luma: u32,
            chroma: u32,
            external: bool,
            source: CameraRect,
            destination: CameraRect,
        ) -> Result<()>;
    }
}
