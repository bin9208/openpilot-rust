#[cxx::bridge(namespace = "product_ui")]
pub(super) mod ffi {
    struct SchedulerResult {
        value: i32,
        error: i32,
    }
    // SAFETY: scalar syscall wrappers retain no memory and return errno by value.
    unsafe extern "C++" {
        include!("scheduling.h");
        fn get_policy(tid: i32) -> SchedulerResult;
        fn set_other(tid: i32) -> SchedulerResult;
    }
}
