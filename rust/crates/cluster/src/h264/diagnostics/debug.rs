use super::super::sps;

#[must_use]
pub fn bytes_head(packet: &[u8], limit: usize) -> String {
    packet
        .iter()
        .take(limit)
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[must_use]
pub fn should_log(index: usize, limit: usize, interval: usize) -> bool {
    index <= limit || (interval > 0 && index.is_multiple_of(interval))
}

#[must_use]
pub fn flags_text(flags: u32, codec_config: bool, keyframe: bool) -> String {
    let mut names = Vec::new();
    let mut known = 0;
    for (value, name) in [
        (0x8, "KEYFRAME"),
        (0x10, "PFRAME"),
        (0x20, "BFRAME"),
        (0x2_0000, "CODECCONFIG"),
        (0x200_0000, "EOS"),
    ] {
        known |= value;
        if flags & value != 0 {
            names.push(name.to_owned());
        }
    }
    if codec_config && flags & 0x2_0000 == 0 {
        names.push("codec_config_cb".to_owned());
    }
    if keyframe && flags & 0x8 == 0 {
        names.push("keyframe_cb".to_owned());
    }
    let extra = flags & !known;
    if extra != 0 {
        names.push(format!("extra=0x{extra:X}"));
    }
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join("|")
    }
}

fn name(kind: u8) -> String {
    match kind {
        1 => "P".to_owned(),
        5 => "IDR".to_owned(),
        6 => "SEI".to_owned(),
        7 => "SPS".to_owned(),
        8 => "PPS".to_owned(),
        9 => "AUD".to_owned(),
        value => format!("NAL{value}"),
    }
}

#[must_use]
pub fn packet_summary(packet: &[u8], max_nals: usize) -> String {
    let nals = sps::nals(packet);
    let mut parts = Vec::new();
    let mut sps_parts = Vec::new();
    let mut largest_name = String::new();
    let mut largest_size = 0;
    for &(_, start, end) in nals.iter().take(max_nals) {
        let kind = packet[start] & 31;
        parts.push(format!("{}:{}", name(kind), end - start));
        if kind == 7 {
            let info = sps::info(&packet[start..end]);
            if !info.is_empty() {
                sps_parts.push(format!("sps[{info}]"));
            }
        }
    }
    for &(_, start, end) in &nals {
        if end - start > largest_size {
            largest_size = end - start;
            largest_name = name(packet[start] & 31);
        }
    }
    if nals.len() > max_nals {
        parts.push(format!("+{}", nals.len() - max_nals));
    }
    if parts.is_empty() {
        return "nals=none".to_owned();
    }
    let mut result = format!(
        "nals={} nal_count={} max={largest_name}:{largest_size}",
        parts.join(","),
        nals.len()
    );
    if !sps_parts.is_empty() {
        result.push(' ');
        result.push_str(&sps_parts.join(" "));
    }
    result
}
