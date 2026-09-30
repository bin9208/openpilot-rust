#[cxx::bridge(namespace = "openpilot_rust")]
pub(crate) mod ffi {
    struct QueueSpec {
        endpoint: String,
        capacity: usize,
        polled: bool,
    }
    #[derive(Debug)]
    struct QueuedMessage {
        index: usize,
        bytes: Vec<u8>,
    }
    // SAFETY: C++ owns value-initialized queues through UniquePtr, copies received
    // bytes into Rust-owned Vec, and never retains the borrowed send slice.
    unsafe extern "C++" {
        include!("bridge.h");
        type Queue;
        type QueueBatch;
        fn open_batch(
            specifications: &[QueueSpec],
            isolated: bool,
        ) -> Result<UniquePtr<QueueBatch>>;
        fn open_queued_batch(specifications: &[QueueSpec]) -> Result<UniquePtr<QueueBatch>>;
        fn receive(self: Pin<&mut QueueBatch>, timeout_ms: i32) -> Result<Vec<QueuedMessage>>;
        fn poll_ready(self: Pin<&mut QueueBatch>, timeout_ms: i32) -> Result<Vec<usize>>;
        fn receive_one(self: Pin<&mut QueueBatch>, index: usize) -> Result<Vec<u8>>;
        fn open_queue(
            endpoint: &str,
            publisher: bool,
            conflate: bool,
            capacity: usize,
        ) -> Result<UniquePtr<Queue>>;
        fn open_runtime_queue(
            endpoint: &str,
            publisher: bool,
            conflate: bool,
            capacity: usize,
        ) -> Result<UniquePtr<Queue>>;
        fn send(self: Pin<&mut Queue>, bytes: &[u8]) -> Result<()>;
        fn readers_caught_up(self: Pin<&mut Queue>) -> bool;
        fn receive(self: Pin<&mut Queue>, timeout_ms: i32) -> Result<Vec<u8>>;
    }
}
