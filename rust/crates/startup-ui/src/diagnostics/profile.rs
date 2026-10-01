use std::{collections::BTreeMap, time::Duration};
#[derive(Default)]
pub struct Profile {
    samples: BTreeMap<&'static str, (u64, Duration)>,
}
impl Profile {
    pub fn record(&mut self, name: &'static str, duration: Duration) {
        let entry = self.samples.entry(name).or_default();
        entry.0 = entry.0.saturating_add(1);
        entry.1 = entry.1.saturating_add(duration);
    }
    pub fn report(&self, limit: usize) -> String {
        let mut entries: Vec<_> = self.samples.iter().collect();
        entries.sort_by_key(|(_, (_, duration))| std::cmp::Reverse(*duration));
        let mut output = String::from("calls\tcumulative_ms\tRust phase\n");
        for (name, (calls, duration)) in entries.into_iter().take(limit) {
            output.push_str(&format!(
                "{calls}\t{:.3}\t{name}\n",
                duration.as_secs_f64() * 1000.0
            ));
        }
        output
    }
}
