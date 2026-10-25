//! Portable service-port listing from the same generated native messaging catalog.
use std::{collections::BTreeSet, fmt::Write};
/// Shape required by the shared generated catalog; values remain in one source file.
pub struct Service {
    pub name: &'static str,
    pub should_log: bool,
    pub frequency: f64,
    pub decimation: Option<u32>,
    pub queue_size: usize,
    pub frequency_range: Option<(f64, f64)>,
}
include!("../../messaging/src/services_generated.rs");
/// Original unsigned FNV-1a over UTF-8 bytes, wrapping after every multiplication.
pub fn fnv1a(endpoint: &str) -> u64 {
    endpoint.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
pub fn port(endpoint: &str) -> u16 {
    (8023 + fnv1a(endpoint) % (65535 - 8023)) as u16
}
/// Match the original set of name/port tuples followed by lexical sorting.
pub fn render<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let rows = names
        .into_iter()
        .map(|name| (name, port(name)))
        .collect::<BTreeSet<_>>();
    let mut output = String::new();
    for (name, port) in rows {
        writeln!(&mut output, "{name} {port}").expect("String writes cannot fail");
    }
    output
}
pub fn catalog_ports() -> String {
    render(SERVICES.iter().map(|service| service.name))
}
