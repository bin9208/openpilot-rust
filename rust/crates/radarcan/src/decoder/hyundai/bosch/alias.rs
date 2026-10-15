//! Publication-only FIFO aliases; raw and physical identities never consume them.
use indexmap::IndexMap;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Serialize)]
pub struct Allocator {
    pub physical_to_alias: IndexMap<u32, u32>,
    pub alias_to_physical: BTreeMap<u32, u32>,
    pub last_published_ns: BTreeMap<u32, i64>,
    pub free_aliases: VecDeque<u32>,
    pub peak_usage: usize,
    pub denial_count: usize,
    pub grace_eviction_count: usize,
}
impl Default for Allocator {
    fn default() -> Self {
        Self {
            physical_to_alias: IndexMap::new(),
            alias_to_physical: BTreeMap::new(),
            last_published_ns: BTreeMap::new(),
            free_aliases: (32..96).collect(),
            peak_usage: 0,
            denial_count: 0,
            grace_eviction_count: 0,
        }
    }
}
impl Allocator {
    fn release(&mut self, pid: u32) {
        let alias = self
            .physical_to_alias
            .shift_remove(&pid)
            .expect("existing publication binding");
        self.alias_to_physical.remove(&alias);
        self.last_published_ns.remove(&pid);
        self.free_aliases.push_back(alias);
    }
    pub fn update(
        &mut self,
        timestamp_ns: i64,
        published_ids: &[u32],
        live_ids: &BTreeSet<u32>,
    ) -> Result<BTreeMap<u32, u32>, &'static str> {
        let published: BTreeSet<_> = published_ids.iter().copied().collect();
        if published.len() > 64 {
            self.denial_count += published.len() - 64;
            return Err("Bosch publication exceeds the front radar alias pool");
        }
        let released: Vec<_> = self
            .physical_to_alias
            .keys()
            .copied()
            .filter(|pid| {
                !live_ids.contains(pid)
                    || i128::from(timestamp_ns) - i128::from(self.last_published_ns[pid])
                        > 500_000_000
            })
            .collect();
        for pid in released {
            self.release(pid);
        }
        let new = published
            .iter()
            .filter(|pid| !self.physical_to_alias.contains_key(*pid))
            .count();
        let needed = new.saturating_sub(self.free_aliases.len());
        if needed > 0 {
            let mut dormant: Vec<_> = self
                .physical_to_alias
                .keys()
                .copied()
                .filter(|pid| !published.contains(pid))
                .collect();
            dormant.sort_by_key(|pid| (self.last_published_ns[pid], *pid));
            for &pid in dormant.iter().take(needed) {
                self.release(pid);
                self.grace_eviction_count += 1;
            }
        }
        for &pid in &published {
            if !self.physical_to_alias.contains_key(&pid) {
                let alias = self
                    .free_aliases
                    .pop_front()
                    .expect("bounded published set leaves enough grace bindings");
                self.physical_to_alias.insert(pid, alias);
                self.alias_to_physical.insert(alias, pid);
            }
            self.last_published_ns.insert(pid, timestamp_ns);
        }
        self.peak_usage = self.peak_usage.max(self.physical_to_alias.len());
        Ok(published
            .into_iter()
            .map(|pid| (pid, self.physical_to_alias[&pid]))
            .collect())
    }
}
