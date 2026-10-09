#[cxx::bridge(namespace = "openpilot_webcam")]
pub(crate) mod ffi {
    struct Info {
        width: f64,
        height: f64,
        fps: f64,
    }
    struct Frame {
        width: u32,
        height: u32,
        data: Vec<u8>,
    }

    // SAFETY: Capture is exclusively owned by UniquePtr and remains on one
    // thread. No borrowed frame escapes C++; row bytes are copied into owned
    // vectors after type and size checks. CXX translates every exception.
    unsafe extern "C++" {
        include!("capture.h");
        type Capture;
        fn open_path(path: &str) -> Result<UniquePtr<Capture>>;
        fn open_index(index: i32) -> Result<UniquePtr<Capture>>;
        fn info(capture: &Capture) -> Result<Info>;
        fn opened(capture: &Capture) -> Result<bool>;
        fn read(capture: Pin<&mut Capture>) -> Result<Frame>;
    }
}
