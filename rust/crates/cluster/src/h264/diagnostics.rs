use num_traits::ToPrimitive;
pub mod debug;

#[derive(Clone, Debug, Default)]
struct Window {
    source: String,
    units: usize,
    key_units: usize,
    unit_bytes: usize,
    max_unit_bytes: usize,
    chunks: usize,
    max_chunks: usize,
    nals: usize,
    max_nals: usize,
    max_nal_bytes: usize,
    queue_max: usize,
    dropped_units: usize,
    dropped_chunks: usize,
    send_chunks: usize,
    send_bytes: usize,
    send_ms: f64,
    send_ms_max: f64,
}

#[derive(Clone, Debug)]
pub struct Diagnostics {
    interval: f64,
    started: f64,
    window: Window,
}

#[must_use]
pub fn nal_stats(packet: &[u8]) -> (usize, usize, bool) {
    let nals = super::sps::nals(packet);
    let maximum = nals
        .iter()
        .map(|&(_, start, end)| end - start)
        .max()
        .unwrap_or(0);
    let idr = nals.iter().any(|&(_, start, _)| packet[start] & 31 == 5);
    (nals.len(), maximum, idr)
}

fn number(value: usize) -> f64 {
    value.to_f64().unwrap_or(f64::MAX)
}

impl Diagnostics {
    #[must_use]
    pub fn new(interval: f64, now: f64) -> Self {
        Self {
            interval,
            started: now,
            window: Window::default(),
        }
    }

    fn enabled(&self) -> bool {
        self.interval > 0.0
    }

    pub fn record_unit(
        &mut self,
        source: &str,
        packet: &[u8],
        chunk_lengths: &[usize],
        keyframe: bool,
    ) {
        if !self.enabled() {
            return;
        }
        let (count, maximum, idr) = nal_stats(packet);
        let window = &mut self.window;
        source.clone_into(&mut window.source);
        window.units = window.units.saturating_add(1);
        window.key_units = window
            .key_units
            .saturating_add(usize::from(keyframe || idr));
        window.unit_bytes = window.unit_bytes.saturating_add(packet.len());
        window.max_unit_bytes = window.max_unit_bytes.max(packet.len());
        window.chunks = window.chunks.saturating_add(chunk_lengths.len());
        window.max_chunks = window.max_chunks.max(chunk_lengths.len());
        window.nals = window.nals.saturating_add(count);
        window.max_nals = window.max_nals.max(count);
        window.max_nal_bytes = window.max_nal_bytes.max(maximum);
    }

    pub fn record_queue_depth(&mut self, depth: usize) {
        if self.enabled() {
            self.window.queue_max = self.window.queue_max.max(depth);
        }
    }

    pub fn record_drop(&mut self, units: usize, chunks: usize) {
        if self.enabled() {
            self.window.dropped_units = self.window.dropped_units.saturating_add(units);
            self.window.dropped_chunks = self.window.dropped_chunks.saturating_add(chunks);
        }
    }

    pub fn record_send(&mut self, source: &str, bytes: usize, milliseconds: f64) {
        if !self.enabled() {
            return;
        }
        let window = &mut self.window;
        source.clone_into(&mut window.source);
        window.send_chunks = window.send_chunks.saturating_add(1);
        window.send_bytes = window.send_bytes.saturating_add(bytes);
        window.send_ms += milliseconds;
        window.send_ms_max = window.send_ms_max.max(milliseconds);
    }

    pub fn report(&mut self, now: f64, backend: &str, force: bool) -> Option<String> {
        if !self.enabled() {
            return None;
        }
        let span = (now - self.started).max(0.001);
        if !force && span < self.interval {
            return None;
        }
        let window = std::mem::take(&mut self.window);
        self.started = now;
        if window.units == 0 && window.send_chunks == 0 {
            return None;
        }
        let units = number(window.units.max(1));
        let unit_average = number(window.unit_bytes) / units;
        let chunks_average = number(window.chunks) / units;
        let nals_average = number(window.nals) / units;
        let unit_kbps = number(window.unit_bytes) * 8.0 / span / 1000.0;
        let send_kbps = number(window.send_bytes) * 8.0 / span / 1000.0;
        let send_average = window.send_ms / number(window.send_chunks.max(1));
        let source = if window.source.is_empty() {
            backend
        } else {
            &window.source
        };
        Some(format!(
            "H264 diag {span:.1}s: backend={backend} source={source} \
             units={} key={} unit_bytes_avg={unit_average:.0} max={} unit_kbps={unit_kbps:.0} \
             chunks_avg={chunks_average:.1} max={} nals_avg={nals_average:.1} max={} \
             max_nal={} au_qmax={} drop_units={} drop_chunks={} send_chunks={} \
             send_kbps={send_kbps:.0} send_ms_avg={send_average:.2} max={:.2}",
            window.units,
            window.key_units,
            window.max_unit_bytes,
            window.max_chunks,
            window.max_nals,
            window.max_nal_bytes,
            window.queue_max,
            window.dropped_units,
            window.dropped_chunks,
            window.send_chunks,
            window.send_ms_max
        ))
    }
}
