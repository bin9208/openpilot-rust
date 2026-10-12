#[cfg(all(
    target_os = "linux",
    target_env = "gnu",
    feature = "allocator-skip-miri"
))]
#[expect(
    unsafe_code,
    reason = "glibc allocator reclamation has a scalar-only system ABI"
)]
pub fn trim() -> bool {
    // SAFETY: Category 8: malloc.h declares int malloc_trim(size_t); usize and c_int match that GNU ABI.
    unsafe extern "C" {
        fn malloc_trim(pad: usize) -> std::ffi::c_int;
    }
    // SAFETY: Category 13: glibc accepts pad=0 without caller-owned pointers and preserves all live allocations.
    unsafe { malloc_trim(0) != 0 }
}

#[cfg(not(all(
    target_os = "linux",
    target_env = "gnu",
    feature = "allocator-skip-miri"
)))]
pub fn trim() -> bool {
    false
}
