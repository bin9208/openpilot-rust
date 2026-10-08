pub fn monotonic() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    num_traits::ToPrimitive::to_f64(&now.tv_sec).unwrap_or(0.)
        + num_traits::ToPrimitive::to_f64(&now.tv_nsec).unwrap_or(0.) / 1e9
}
pub fn timestamp() -> Result<u64, crate::Error> {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    u64::try_from(now.tv_sec)
        .ok()
        .and_then(|s| s.checked_mul(1_000_000_000))
        .and_then(|s| s.checked_add(u64::try_from(now.tv_nsec).ok()?))
        .ok_or(crate::Error::Contract("monotonic timestamp overflow"))
}
pub fn wall() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
}
