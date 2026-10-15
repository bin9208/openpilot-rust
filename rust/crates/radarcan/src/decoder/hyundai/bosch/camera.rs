//! Pinned Bosch camera cycle assembly and causal snapshots.
const SLOTS: usize = 10;
const SNAPSHOTS: usize = 4;
const PERIOD: i64 = 40_000_000;
const TOLERANCE: i64 = 25_000_000;
const GAP: i64 = 160_000_000;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraObject {
    pub obj_id: usize,
    pub episode: i64,
    pub long_m: f64,
    pub lat_m: f64,
    pub vrel_mps: f64,
    pub width_m: f64,
    pub class_code: i64,
    pub angle_left: f64,
    pub angle_right: f64,
}
impl Default for CameraObject {
    fn default() -> Self {
        Self {
            obj_id: 0,
            episode: 0,
            long_m: 0.,
            lat_m: 0.,
            vrel_mps: 0.,
            width_m: 0.,
            class_code: -1,
            angle_left: 0.,
            angle_right: 0.,
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct CameraCycleCache {
    _counter: i64,
    _cycle_index: i64,
    _header_ns: i64,
    _n_objects: usize,
    _a: Vec<u64>,
    _b: Vec<u64>,
    _c: Vec<u64>,
    _a_ns: Vec<i64>,
    _b_ns: Vec<i64>,
    _c_ns: Vec<i64>,
    _masks: [u16; 3],
    _pending: Vec<Vec<u64>>,
    _pending_ns: Vec<Vec<i64>>,
    _buffers: Vec<Vec<CameraObject>>,
    _snapshot_cycle: [i64; SNAPSHOTS],
    _snapshot_header_ns: [i64; SNAPSHOTS],
    _snapshot_complete_ns: [i64; SNAPSHOTS],
    _snapshot_count: [usize; SNAPSHOTS],
    _snapshot_next: usize,
    _last_seen_cycle: Vec<i64>,
    _episode: Vec<i64>,
    _next_episode: i64,
    pub fault_ns: i64,
    pub completed_cycles: u64,
    pub rejected_cycles: u64,
}
impl Default for CameraCycleCache {
    fn default() -> Self {
        Self {
            _counter: -1,
            _cycle_index: -1,
            _header_ns: -1,
            _n_objects: 0,
            _a: vec![0; SLOTS],
            _b: vec![0; SLOTS],
            _c: vec![0; SLOTS],
            _a_ns: vec![-1; SLOTS],
            _b_ns: vec![-1; SLOTS],
            _c_ns: vec![-1; SLOTS],
            _masks: [0; 3],
            _pending: vec![vec![0; 16 * SLOTS]; 3],
            _pending_ns: vec![vec![-1; 16 * SLOTS]; 3],
            _buffers: vec![vec![CameraObject::default(); SLOTS]; SNAPSHOTS],
            _snapshot_cycle: [-1; SNAPSHOTS],
            _snapshot_header_ns: [-1; SNAPSHOTS],
            _snapshot_complete_ns: [-1; SNAPSHOTS],
            _snapshot_count: [0; SNAPSHOTS],
            _snapshot_next: 0,
            _last_seen_cycle: vec![-1000; 256],
            _episode: vec![0; 256],
            _next_episode: 0,
            fault_ns: -1,
            completed_cycles: 0,
            rejected_cycles: 0,
        }
    }
}
fn signed(value: u64, bits: u32) -> i64 {
    let value = i64::try_from(value).expect("masked camera word");
    if value & (1 << (bits - 1)) != 0 {
        value - (1 << bits)
    } else {
        value
    }
}
impl CameraCycleCache {
    fn unwrap(&self, counter: i64, timestamp: i64) -> i64 {
        if self._counter < 0 {
            return 0;
        }
        let elapsed = (((timestamp - self._header_ns) as f64 / PERIOD as f64).round_ties_even()
            as i64)
            .max(0);
        let wanted = (counter - self._counter) & 15;
        let mut step = elapsed + ((wanted - elapsed) & 15);
        if step - elapsed > 8 {
            step -= 16;
        }
        self._cycle_index + step.max(wanted)
    }
    fn store(&mut self, kind: usize, slot: usize, word: u64, timestamp: i64) {
        match kind {
            0 => {
                self._a[slot] = word;
                self._a_ns[slot] = timestamp;
            }
            1 => {
                self._b[slot] = word;
                self._b_ns[slot] = timestamp;
            }
            _ => {
                self._c[slot] = word;
                self._c_ns[slot] = timestamp;
            }
        }
        self._masks[kind] |= 1 << slot;
    }
    fn complete(&self) -> bool {
        let required = (1 << self._n_objects) - 1;
        self._masks.iter().all(|mask| mask & required == required)
    }
    fn try_complete(&mut self) {
        if self._counter < 0
            || !self.complete()
            || self._snapshot_cycle.contains(&self._cycle_index)
        {
            return;
        }
        let mut seen = [false; 256];
        for slot in 0..self._n_objects {
            let a = self._a[slot];
            let id = usize::try_from(a & 255).expect("masked camera ID");
            if a & ((1 << 48) - 1) == 0 || seen[id] {
                self.fault_ns = self._header_ns.max(self._a_ns[slot]);
                self.rejected_cycles += 1;
                return;
            }
            seen[id] = true;
        }
        let target = self._snapshot_next;
        let mut completed = self._header_ns;
        for slot in 0..self._n_objects {
            let (a, b, c) = (self._a[slot], self._b[slot], self._c[slot]);
            completed = completed
                .max(self._a_ns[slot])
                .max(self._b_ns[slot])
                .max(self._c_ns[slot]);
            let id = usize::try_from(a & 255).expect("masked camera ID");
            if self._cycle_index - self._last_seen_cycle[id] > 2 {
                self._next_episode += 1;
                self._episode[id] = self._next_episode;
            }
            self._last_seen_cycle[id] = self._cycle_index;
            self._buffers[target][slot] = CameraObject {
                obj_id: id,
                episode: self._episode[id],
                long_m: ((a >> 8) & 0xfff) as f64 * 0.05,
                lat_m: signed((a >> 20) & 0xfff, 12) as f64 * 0.05,
                vrel_mps: signed((a >> 40) & 0xfff, 12) as f64 * 0.05,
                width_m: (b & 0x3f) as f64 * 0.05,
                class_code: ((b >> 48) & 3) as i64 + 4 * ((b >> 50) & 1) as i64,
                angle_right: signed((c >> 18) & 0x1fff, 13) as f64 * (1. / 4496.3),
                angle_left: signed((c >> 31) & 0x1fff, 13) as f64 * (1. / 4496.3),
            };
        }
        self._snapshot_cycle[target] = self._cycle_index;
        self._snapshot_header_ns[target] = self._header_ns;
        self._snapshot_complete_ns[target] = completed;
        self._snapshot_count[target] = self._n_objects;
        self._snapshot_next = (target + 1) % SNAPSHOTS;
        self.completed_cycles += 1;
    }
    fn start(&mut self, counter: i64, count: usize, timestamp: i64) -> bool {
        if self._header_ns >= 0 && timestamp <= self._header_ns {
            return false;
        }
        if self._counter >= 0 && self._n_objects != 0 && !self.complete() {
            self.fault_ns = timestamp;
            self.rejected_cycles += 1;
        }
        let cycle = self.unwrap(counter, timestamp);
        self._counter = counter;
        self._cycle_index = cycle;
        self._header_ns = timestamp;
        self._n_objects = count;
        self._masks = [0; 3];
        self._a_ns.fill(-1);
        self._b_ns.fill(-1);
        self._c_ns.fill(-1);
        for kind in 0..3 {
            for slot in 0..SLOTS {
                let index = usize::try_from(counter).expect("masked counter") * SLOTS + slot;
                let pending = self._pending_ns[kind][index];
                if pending >= 0 && (pending - timestamp).abs() <= TOLERANCE {
                    self.store(kind, slot, self._pending[kind][index], pending);
                }
                self._pending_ns[kind][index] = -1;
            }
        }
        self.try_complete();
        true
    }
    pub fn ingest(&mut self, timestamp: i64, address: u32, payload: &[u8]) -> bool {
        let Ok(bytes) = <&[u8; 8]>::try_from(payload) else {
            self.fault_ns = timestamp;
            return false;
        };
        let word = u64::from_le_bytes(*bytes);
        if address == 0x738 {
            let count = usize::try_from(word & 15).expect("masked count");
            if count > SLOTS {
                self.fault_ns = timestamp;
                self.rejected_cycles += 1;
                return false;
            }
            return self.start(((word >> 52) & 15) as i64, count, timestamp);
        }
        if !(0x739..=0x756).contains(&address) {
            return false;
        }
        let index = usize::try_from(address - 0x739).expect("camera address");
        let (slot, kind) = (index / 3, index % 3);
        let counter = ((word >> 52) & 15) as i64;
        if counter == self._counter
            && self._header_ns >= 0
            && (timestamp - self._header_ns).abs() <= TOLERANCE
        {
            self.store(kind, slot, word, timestamp);
            if self.complete() && !self._snapshot_cycle.contains(&self._cycle_index) {
                self.try_complete();
            }
        } else {
            let index = usize::try_from(counter).expect("masked counter") * SLOTS + slot;
            if timestamp >= self._pending_ns[kind][index] {
                self._pending[kind][index] = word;
                self._pending_ns[kind][index] = timestamp;
            }
        }
        true
    }
    pub fn snapshot(&self, timestamp: i64) -> Option<(&[CameraObject], usize, i64, i64)> {
        let mut best = None;
        let mut cycle = -1;
        for index in 0..SNAPSHOTS {
            let completed = self._snapshot_complete_ns[index];
            if completed <= timestamp
                && completed > self.fault_ns
                && timestamp - completed <= GAP
                && self._snapshot_cycle[index] > cycle
            {
                best = Some(index);
                cycle = self._snapshot_cycle[index];
            }
        }
        best.map(|index| {
            (
                self._buffers[index].as_slice(),
                self._snapshot_count[index],
                self._snapshot_cycle[index],
                self._snapshot_complete_ns[index],
            )
        })
    }
}
