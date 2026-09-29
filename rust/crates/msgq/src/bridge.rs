#[cxx::bridge(namespace = "openpilot_rust")]
pub(crate) mod ffi {
    // SAFETY: C++ owns value-initialized queues through UniquePtr, copies received
    // bytes into Rust-owned Vec, and never retains the borrowed send slice.
    unsafe extern "C++" {
        include!("bridge.h");
        type Queue;
        fn open_queue(endpoint: &str, publisher: bool, conflate: bool) -> Result<UniquePtr<Queue>>;
        fn send(self: Pin<&mut Queue>, bytes: &[u8]) -> Result<()>;
        fn receive(self: Pin<&mut Queue>, timeout_ms: i32) -> Result<Vec<u8>>;
    }
}
