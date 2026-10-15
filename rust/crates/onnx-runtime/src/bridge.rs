#[cxx::bridge(namespace = "openpilot_ort")]
pub(crate) mod ffi {
    struct Tensor {
        shape: Vec<i64>,
        values: Vec<f32>,
    }
    // SAFETY: the adapter checks name/shape/slice contracts independently,
    // consumes input synchronously and returns owned copies. Only verified
    // provider ELFs are passed to load. CXX translates exceptions to Result.
    unsafe extern "C++" {
        include!("bridge.h");
        type Session;
        fn load(library: &str, model: &str) -> Result<UniquePtr<Session>>;
        fn run(
            session: Pin<&mut Session>,
            name: &str,
            input: &[f32],
            shape: &[i64],
        ) -> Result<Vec<Tensor>>;
    }
}
