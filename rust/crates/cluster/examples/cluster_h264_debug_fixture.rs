use openpilot_cluster::h264::{diagnostics::debug, sps};
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    packets: Vec<Vec<u8>>,
    flags: Vec<(u32, bool, bool)>,
    gates: Vec<(usize, usize, usize)>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Input = serde_json::from_reader(std::io::stdin().lock())?;
    let packets: Vec<_> = input
        .packets
        .iter()
        .map(|packet| {
            let heads: Vec<_> = [0, 8, 16, 32]
                .map(|limit| debug::bytes_head(packet, limit))
                .into();
            let summaries: Vec<_> = [0, 1, 6, 10]
                .map(|limit| debug::packet_summary(packet, limit))
                .into();
            let sps: Vec<_> = sps::nals(packet)
                .iter()
                .map(|&(_, start, end)| sps::info(&packet[start..end]))
                .collect();
            serde_json::json!({"heads":heads,"summaries":summaries,"sps":sps})
        })
        .collect();
    let flags: Vec<_> = input
        .flags
        .into_iter()
        .map(|(value, codec, key)| debug::flags_text(value, codec, key))
        .collect();
    let gates: Vec<_> = input
        .gates
        .into_iter()
        .map(|(index, limit, interval)| debug::should_log(index, limit, interval))
        .collect();
    println!(
        "{}",
        serde_json::json!({"packets":packets,"flags":flags,"gates":gates})
    );
    Ok(())
}
