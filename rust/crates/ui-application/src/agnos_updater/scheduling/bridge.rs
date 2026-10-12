#[cxx::bridge(namespace = "agnos_updater")]
pub(super) mod ffi {
    struct Result {
        error: i32,
    }
    // SAFETY: scalar syscall wrapper retains no memory and returns errno by value.
    unsafe extern "C++" {
        include!("updater_scheduling.h");
        fn set_fifo(priority: i32) -> Result;
    }
}
