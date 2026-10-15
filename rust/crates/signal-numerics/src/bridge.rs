#[cxx::bridge(namespace = "openpilot_signal_numeric")]
pub(crate) mod ffi {
    // SAFETY: Rust creates one index per input before the synchronous call;
    // C++ retains no pointers and the pinned primitive only permutes indices.
    unsafe extern "C++" {
        include!("bridge.h");
        fn sort_v3(values: &[f32], indices: &mut [usize]);
        fn sort_v4(values: &[f32], indices: &mut [usize]);
    }
}

pub(super) fn has_lahf() -> bool {
    // Query an extended leaf only after checking its advertised maximum.
    std::arch::x86_64::__cpuid(0x8000_0000).eax >= 0x8000_0001
        && std::arch::x86_64::__cpuid(0x8000_0001).ecx & 1 != 0
}
