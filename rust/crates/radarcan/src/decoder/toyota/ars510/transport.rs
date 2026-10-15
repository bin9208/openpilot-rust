//! Segmented 0x80 / 0x85 record assembly, preserving the source reset policy.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct CompletedRecord {
    pub time_s: f64,
    pub payload: Vec<u8>,
    pub frame_count: usize,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SegmentedRecordAssembler {
    first_frame: [u8; 2],
    frames_per_record: usize,
    record_len: usize,
    _chunks: Vec<Vec<u8>>,
    _start_time_s: Option<f64>,
    _last_time_s: Option<f64>,
}
impl SegmentedRecordAssembler {
    pub fn id80() -> Self {
        Self::new([0x12, 0xe4], 106, 742)
    }
    pub fn id85() -> Self {
        Self::new([0x10, 0x90], 21, 147)
    }
    fn new(first_frame: [u8; 2], frames_per_record: usize, record_len: usize) -> Self {
        Self {
            first_frame,
            frames_per_record,
            record_len,
            _chunks: Vec::new(),
            _start_time_s: None,
            _last_time_s: None,
        }
    }
    pub fn reset(&mut self) {
        self._chunks.clear();
        self._start_time_s = None;
        self._last_time_s = None;
    }
    pub fn push(&mut self, time_s: f64, data: &[u8]) -> Option<CompletedRecord> {
        if data.len() != 8 || !time_s.is_finite() {
            self.reset();
            return None;
        }
        if !self._chunks.is_empty() && self._last_time_s.is_some_and(|previous| time_s < previous) {
            self.reset();
        }
        if data[..2] == self.first_frame {
            self._chunks.clear();
            self._chunks.push(data[1..].to_vec());
            self._start_time_s = Some(time_s);
        } else if !self._chunks.is_empty() {
            self._chunks.push(data[1..].to_vec());
        } else {
            return None;
        }
        self._last_time_s = Some(time_s);
        if self._chunks.len() != self.frames_per_record {
            return None;
        }
        let payload: Vec<_> = self._chunks.iter().flatten().copied().collect();
        let time_s = self._start_time_s.unwrap_or(time_s);
        let frame_count = self._chunks.len();
        self.reset();
        if payload.len() != self.record_len {
            return None;
        }
        Some(CompletedRecord {
            time_s,
            payload,
            frame_count,
        })
    }
}

/// Carrot's strict 0x80 receiver wrapper; counters and previous input time survive reset.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StrictId80RecordAssembler {
    pub counters: std::collections::BTreeMap<String, u64>,
    pub rejections: u64,
    _input_time_s: Option<f64>,
    #[serde(flatten)]
    inner: SegmentedRecordAssembler,
}
impl Default for StrictId80RecordAssembler {
    fn default() -> Self {
        Self {
            counters: std::collections::BTreeMap::new(),
            rejections: 0,
            _input_time_s: None,
            inner: SegmentedRecordAssembler::id80(),
        }
    }
}
impl StrictId80RecordAssembler {
    pub fn reset(&mut self) {
        self.inner.reset();
    }
    fn count(&mut self, reason: &str) {
        *self.counters.entry(reason.into()).or_default() += 1;
    }
    fn reject(&mut self, reason: &str) {
        self.count(reason);
        self.rejections += 1;
        self.reset();
    }
    pub fn push(&mut self, time_s: f64, data: &[u8]) -> Option<CompletedRecord> {
        if !time_s.is_finite() {
            self.reject("nonfinite_time");
            return None;
        }
        if self._input_time_s.is_some_and(|previous| time_s < previous) {
            self.reject("reversed_time");
            self._input_time_s = Some(time_s);
            return None;
        }
        self._input_time_s = Some(time_s);
        if data.len() != 8 {
            self.reject("bad_frame_length");
            return None;
        }
        if data[..2] == self.inner.first_frame {
            if !self.inner._chunks.is_empty() {
                self.count("interrupted_records");
            }
            self.count("starts");
            return self.inner.push(time_s, data);
        }
        if data[0] >> 4 != 2 {
            self.reject("bad_frame_type");
            return None;
        }
        if self.inner._chunks.is_empty() {
            self.count("orphan_continuations");
            return None;
        }
        if time_s - self.inner._start_time_s.expect("active record start") > 0.15 {
            self.reject("assembly_timeout");
            return None;
        }
        if usize::from(data[0] & 15) != (self.inner._chunks.len() & 15) {
            self.reject("sequence_errors");
            return None;
        }
        let record = self.inner.push(time_s, data);
        if record.is_some() {
            self.count("completed_records");
        }
        record
    }
}
