#[path = "../src/idle_trim/allocator.rs"]
mod allocator;

fn main() {
    let live = vec![29_u8; if cfg!(miri) { 64 } else { 1024 * 1024 }];
    let released = allocator::trim();
    assert!(live.iter().all(|byte| *byte == 29));
    let concurrent = std::thread::spawn(|| {
        for _ in 0..64 {
            let active = vec![31_u8; if cfg!(miri) { 64 } else { 64 * 1024 }];
            allocator::trim();
            assert!(active.iter().all(|byte| *byte == 31));
        }
    });
    for _ in 0..64 {
        allocator::trim();
    }
    concurrent.join().unwrap();
    let ffi_called = cfg!(all(
        target_os = "linux",
        target_env = "gnu",
        feature = "allocator-skip-miri"
    ));
    println!("{{\"ffi_called\":{ffi_called},\"live_allocation_preserved\":true,\"concurrent_calls\":128,\"released\":{released}}}");
}
