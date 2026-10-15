//! Optional pinned x86 numeric primitive; no inference or signal policy.
#[cfg(all(feature = "native-skip-miri", target_arch = "x86_64"))]
#[expect(
    unsafe_code,
    reason = "borrowed-slice CXX boundary to authenticated external numeric code"
)]
mod bridge;
/// Return the source x86 sort when its CPU dispatch is available. Callers own
/// the source portable fallback on other architectures or scalar-only CPUs.
pub fn argsort(values: &[f32]) -> Option<Vec<usize>> {
    #[cfg(all(feature = "native-skip-miri", target_arch = "x86_64"))]
    {
        let v3 = std::is_x86_feature_detected!("sse3")
            && std::is_x86_feature_detected!("ssse3")
            && std::is_x86_feature_detected!("sse4.1")
            && std::is_x86_feature_detected!("sse4.2")
            && std::is_x86_feature_detected!("popcnt")
            && std::is_x86_feature_detected!("cmpxchg16b")
            && bridge::has_lahf()
            && std::is_x86_feature_detected!("avx2")
            && std::is_x86_feature_detected!("fma")
            && std::is_x86_feature_detected!("bmi1")
            && std::is_x86_feature_detected!("bmi2")
            && std::is_x86_feature_detected!("lzcnt")
            && std::is_x86_feature_detected!("f16c")
            && std::is_x86_feature_detected!("movbe");
        let mut order = (0..values.len()).collect::<Vec<_>>();
        if v3
            && std::is_x86_feature_detected!("avx512f")
            && std::is_x86_feature_detected!("avx512bw")
            && std::is_x86_feature_detected!("avx512cd")
            && std::is_x86_feature_detected!("avx512dq")
            && std::is_x86_feature_detected!("avx512vl")
        {
            bridge::ffi::sort_v4(values, &mut order);
            return Some(order);
        }
        if v3 {
            bridge::ffi::sort_v3(values, &mut order);
            return Some(order);
        }
    }
    let _ = values;
    None
}
