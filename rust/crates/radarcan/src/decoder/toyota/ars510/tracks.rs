//! ARS510 slot/age allocation lifecycle, including chronological/idempotent updates.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct NativeTrackIdAssigner {
    pub max_gap_s: f64,
    _state: std::collections::BTreeMap<i64, (u64, i64, f64)>,
    _next_id: u64,
    _last_time_s: Option<f64>,
}
impl Default for NativeTrackIdAssigner {
    fn default() -> Self {
        Self::new(0.3)
    }
}
impl NativeTrackIdAssigner {
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self._state.values().map(|state| state.0)
    }
    pub fn new(max_gap_s: f64) -> Self {
        Self {
            max_gap_s,
            _state: std::collections::BTreeMap::new(),
            _next_id: 1,
            _last_time_s: None,
        }
    }
    pub fn update(&mut self, time_s: f64, slot: i64, age: i64) -> Result<u64, &'static str> {
        if !time_s.is_finite() {
            return Err("time_s must be finite");
        }
        if self._last_time_s.is_some_and(|previous| time_s < previous) {
            return Err("track updates must be chronological; reset per route or segment");
        }
        let previous = self._state.get(&slot).copied();
        if let Some(previous) = previous {
            if time_s == previous.2 {
                if age != previous.1 {
                    return Err("conflicting ages for the same slot and timestamp");
                }
                return Ok(previous.0);
            }
        }
        let continues = previous.is_some_and(|previous| {
            time_s - previous.2 <= self.max_gap_s
                && (age > previous.1 || (age == 126 && previous.1 == 126))
        });
        let id = if continues {
            previous.expect("continuing allocation").0
        } else {
            let id = self._next_id;
            self._next_id += 1;
            id
        };
        self._state.insert(slot, (id, age, time_s));
        self._last_time_s = Some(time_s);
        Ok(id)
    }
}
