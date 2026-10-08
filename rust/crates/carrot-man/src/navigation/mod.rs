mod legacy;
mod projection;
pub mod v2;
use crate::sources::{Control, Snapshot, SourceStore};
pub use legacy::{parse_legacy, Auxiliary, LegacyFields, Traffic};
pub use projection::Projected;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct NavigationRuntime {
    pub store: SourceStore,
    legacy_sequence: u64,
    legacy_snapshot: Option<Snapshot>,
    legacy_road_limits: BTreeMap<String, (f64, Option<f64>, u64)>,
    legacy_controls: BTreeMap<String, Control>,
    legacy_pending: Vec<(String, legacy::Auxiliary)>,
    v2_session: String,
    v2_sequence: u64,
    v2_items: BTreeMap<&'static str, (u64, f64)>,
    projector: projection::Projector,
}

impl NavigationRuntime {
    pub fn accept(&mut self, mut snapshot: Snapshot) -> bool {
        if snapshot.source == crate::sources::Source::NaverV1 && snapshot.control.route_present {
            snapshot.control.status_present = true;
            snapshot.control.status_received_mono_s = snapshot.control.route_received_mono_s;
        }
        let now = snapshot.received_mono_s;
        self.store.accept(snapshot, now)
    }

    pub fn select(&mut self, now: f64) -> Result<Projected, &'static str> {
        let selection = self.store.select(now)?;
        Ok(self.projector.project(selection))
    }
}
