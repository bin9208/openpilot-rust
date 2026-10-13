use super::fmp4::{Initialization, Muxer, Output, Segment};

impl Muxer {
    pub(super) fn consume(&mut self, bytes: &[u8]) -> Result<Output, String> {
        let mut initialization = Vec::new();
        let mut segments = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() {
            let header = bytes
                .get(offset..offset + 8)
                .ok_or("incomplete fragmented MP4 box header")?;
            let mut size = usize::try_from(u32::from_be_bytes(
                header[..4].try_into().map_err(|_| "invalid box size")?,
            ))
            .map_err(|error| error.to_string())?;
            let kind = &header[4..8];
            let minimum = if size == 1 {
                let extended = bytes
                    .get(offset + 8..offset + 16)
                    .ok_or("incomplete extended fragmented MP4 box header")?;
                size = usize::try_from(u64::from_be_bytes(
                    extended.try_into().map_err(|_| "invalid box size")?,
                ))
                .map_err(|error| error.to_string())?;
                16
            } else {
                if size == 0 {
                    size = bytes.len() - offset;
                }
                8
            };
            let end = offset
                .checked_add(size)
                .ok_or("invalid fragmented MP4 box size")?;
            if size < minimum {
                return Err("invalid fragmented MP4 box size".into());
            }
            let boxed = bytes
                .get(offset..end)
                .ok_or("invalid fragmented MP4 box size")?;
            match kind {
                b"ftyp" | b"moov" => initialization.extend_from_slice(boxed),
                b"moof" => {
                    if !self.fragment.is_empty() {
                        return Err("fragmented MP4 media fragment is missing mdat".into());
                    }
                    self.fragment.extend_from_slice(boxed);
                }
                b"mdat" => {
                    if self.fragment.is_empty() {
                        return Err("fragmented MP4 media data has no pending sample".into());
                    }
                    let sample = self
                        .pending
                        .pop_front()
                        .ok_or("fragmented MP4 media data has no pending sample")?;
                    self.fragment.extend_from_slice(boxed);
                    segments.push(Segment {
                        payload: std::mem::take(&mut self.fragment),
                        sequence: sample.sequence,
                        timestamp_ms: sample.timestamp_ms,
                        duration_ms: sample.duration_ms,
                        keyframe: sample.keyframe,
                    });
                }
                b"mfra" => {}
                _ => {
                    if !self.fragment.is_empty() {
                        self.fragment.extend_from_slice(boxed);
                    }
                }
            }
            offset = end;
        }
        let initialization = if initialization.is_empty() || self.initialized {
            None
        } else {
            self.initialized = true;
            Some(Initialization {
                payload: initialization,
                mime: format!("video/mp4; codecs=\"{}\"", codec(&self.config)),
                width: self.dimensions.0,
                height: self.dimensions.1,
            })
        };
        Ok(Output {
            initialization,
            segments,
        })
    }
}
fn codec(bytes: &[u8]) -> String {
    let mut offset = 0;
    while offset + 3 <= bytes.len() {
        let prefix = if bytes.get(offset..offset + 4) == Some(b"\0\0\0\x01") {
            4
        } else if bytes.get(offset..offset + 3) == Some(b"\0\0\x01") {
            3
        } else {
            offset += 1;
            continue;
        };
        let start = offset + prefix;
        if let Some(sps) = bytes
            .get(start..start + 4)
            .filter(|bytes| bytes[0] & 0x1f == 7)
        {
            return format!("avc1.{:02X}{:02X}{:02X}", sps[1], sps[2], sps[3]);
        }
        offset = start;
    }
    "avc1.42E01E".into()
}
