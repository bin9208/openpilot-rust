#[cxx::bridge(namespace = "openpilot_jpeg")]
pub(crate) mod ffi {
    struct Layout {
        width: u32,
        height: u32,
        components: u8,
    }
    // SAFETY: category8/14 FFI guards validate packed dimensions/length/quality
    // before reading the immutable borrowed slice and retain no pixel pointer.
    // Native longjmp stays inside C; exceptions cross via CXX Result. The owned
    // nonnull Turbo resource pins its DLL/handle until destruction on its owner thread.
    unsafe extern "C++" {
        include!("bridge.h");
        include!("turbo.h");
        type Turbo;
        fn open_turbo(path: &[u8]) -> Result<UniquePtr<Turbo>>;
        fn encode_turbo(
            codec: Pin<&mut Turbo>,
            pixels: &[u8],
            layout: Layout,
            quality: u8,
        ) -> Result<Vec<u8>>;
        fn encode(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>>;
        fn encode_with(pixels: &[u8], layout: Layout, quality: u8) -> Result<Vec<u8>>;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;
    #[test]
    fn native_layout_and_quality_guard_precedes_codec_pointer_use() {
        assert!(ffi::encode_with(
            &[],
            ffi::Layout {
                width: 4,
                height: 2,
                components: 2
            },
            75
        )
        .is_err());
        assert!(ffi::encode_with(
            &[0; 24],
            ffi::Layout {
                width: 4,
                height: 2,
                components: 3
            },
            0
        )
        .is_err());
        assert!(ffi::encode_with(
            &[0; 24],
            ffi::Layout {
                width: 4,
                height: 2,
                components: 3
            },
            101
        )
        .is_err());
        assert!(ffi::encode_with(
            &[0; 7],
            ffi::Layout {
                width: 4,
                height: 2,
                components: 1
            },
            50
        )
        .is_err());
    }
}
