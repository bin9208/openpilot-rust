use super::State;
use crate::h264::{diagnostics::debug, sps, Config};

pub(super) fn session(state: &State, config: &Config, display: [i32; 2], requested: &str) {
    if state.debug {
        println!(
            "H264 debug session: backend={} requested_backend={requested} display={}x{} encoder={}x{} fps={} bitrate={} gop={} chunk_size={} requested_chunk={} packetize_mode=access-unit wait_ack={} soft_ack={} dump={}",
            state.backend,
            display[0],
            display[1],
            config.width,
            config.height,
            config.fps,
            config.bitrate,
            config.gop,
            state.chunk_size,
            config.requested_chunk_size,
            u8::from(config.wait_for_ack),
            u8::from(config.soft_ack),
            config
                .dump
                .as_ref()
                .map_or_else(|| "off".into(), |path| path.display().to_string())
        );
    }
}
pub(super) fn unit(state: &mut State, packet: &[u8], chunks: &[usize]) {
    let counts = &mut state.counts;
    let index = if state.backend == "ffmpeg" {
        counts.stdout_reads += 1;
        counts.stdout_bytes += packet.len();
        counts.stdout_reads
    } else {
        counts.encoder_packets
    };
    if !state.debug || !debug::should_log(index, 40, 30) {
        return;
    }
    if state.backend == "ffmpeg" {
        println!(
            "H264 stdout read {index}: {} size={} ffmpeg_read=1 head={} {}",
            state.backend,
            packet.len(),
            debug::bytes_head(packet, 16),
            debug::packet_summary(packet, 10)
        );
    }
    let first = packet
        .get(..chunks.first().copied().unwrap_or(0))
        .unwrap_or_default();
    let last = packet
        .get(
            packet
                .len()
                .saturating_sub(chunks.last().copied().unwrap_or(0))..,
        )
        .unwrap_or_default();
    println!(
        "H264 packetize {index}: {} mode=access-unit input={} chunk_limit={} chunks={} sizes={}/{}/{} start_units={} first={} last={}",
        state.backend,
        packet.len(),
        state.chunk_size,
        chunks.len(),
        chunks.iter().min().unwrap_or(&0),
        chunks.iter().max().unwrap_or(&0),
        chunks.iter().sum::<usize>(),
        sps::nals(packet).len(),
        debug::bytes_head(first, 8),
        debug::bytes_head(last, 8)
    );
}
pub(super) fn sent(state: &State, bytes: &[u8]) {
    if state.debug && debug::should_log(state.counts.sends, 60, 25) {
        println!(
            "H264 chunk {}: {} {} bytes limit={} head={} ack={} last=0 total_usb={} {}",
            state.counts.sends,
            state.backend,
            bytes.len(),
            state.chunk_size,
            debug::bytes_head(bytes, 8),
            state.ack,
            state.counts.send_bytes,
            debug::packet_summary(bytes, 6)
        );
    }
}
pub(super) fn finish(state: &State) {
    if state.debug {
        let counts = &state.counts;
        println!(
            "H264 debug summary: backend={} elapsed={:.2}s encoder_packets={} encoder_bytes={} stdout_reads={} stdout_bytes={} packetize_events={} usb_chunks={} usb_bytes={} dropped_units={} dropped_chunks={} max_packet={} max_chunk={}",
            state.backend,
            state.started.elapsed().as_secs_f64().max(0.001),
            counts.encoder_packets,
            counts.encoder_bytes,
            counts.stdout_reads,
            counts.stdout_bytes,
            counts.units,
            counts.sends,
            counts.send_bytes,
            counts.drops,
            counts.drop_chunks,
            counts.max_packet,
            counts.max_chunk
        );
    }
}
#[cfg(feature = "native-media")]
pub(super) fn packet(
    state: &State,
    packet: &crate::h264::vidc::Packet,
    bytes: &[u8],
    patched: bool,
) {
    let index = state.counts.encoder_packets;
    if state.debug && debug::should_log(index, 40, 30) {
        println!(
            "H264 encoder packet {index}: native/{} size={} flags=0x{:08X}({}) ts={} codec_config={} keyframe={} head={} {}",
            if patched { "patched" } else { "raw" },
            bytes.len(),
            packet.flags,
            debug::flags_text(packet.flags, packet.codec_config, packet.keyframe),
            packet.timestamp_us,
            u8::from(packet.codec_config),
            u8::from(packet.keyframe),
            debug::bytes_head(bytes, 16),
            debug::packet_summary(bytes, 10)
        );
    }
}
