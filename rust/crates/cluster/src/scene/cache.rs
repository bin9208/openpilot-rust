use std::collections::VecDeque;

pub struct Cache<K, V> {
    entries: VecDeque<(K, V)>,
    limit: usize,
}

impl<K: PartialEq, V: Clone> Cache<K, V> {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit,
        }
    }
    #[must_use]
    pub fn get(&mut self, key: &K) -> Option<V> {
        self.get_if(key, |_| true)
    }
    #[must_use]
    pub fn get_if(&mut self, key: &K, accept: impl FnOnce(&V) -> bool) -> Option<V> {
        let index = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == key)?;
        if !accept(&self.entries[index].1) {
            return None;
        }
        let entry = self.entries.remove(index)?;
        let value = entry.1.clone();
        self.entries.push_back(entry);
        Some(value)
    }
    pub fn insert(&mut self, key: K, value: V) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(candidate, _)| candidate == &key)
        {
            self.entries[index].1 = value;
            return;
        }
        self.entries.push_back((key, value));
        while self.entries.len() > self.limit {
            self.entries.pop_front();
        }
    }
}

#[must_use]
pub fn cache_grid_value(value: f64, grid: f64) -> f64 {
    (value / grid).round_ties_even() * grid
}
#[must_use]
pub fn model_line_cache_start_m(value: f64) -> f64 {
    (value / 0.5).floor() * 0.5
}
#[must_use]
pub fn model_line_cache_end_m(value: f64) -> f64 {
    (value / 0.5).ceil() * 0.5
}
