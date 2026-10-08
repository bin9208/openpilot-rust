use super::{Lifecycle, Snapshot, SourceStore};

fn activation_choice(values: &[Snapshot]) -> Option<Snapshot> {
    values
        .iter()
        .min_by(|a, b| {
            b.activation_epoch
                .cmp(&a.activation_epoch)
                .then(a.source.cmp(&b.source))
        })
        .cloned()
}
fn fallback_choice(values: &[Snapshot]) -> Option<Snapshot> {
    values
        .iter()
        .min_by(|a, b| {
            b.owner_receipt()
                .total_cmp(&a.owner_receipt())
                .then(a.source.cmp(&b.source))
        })
        .cloned()
}

impl SourceStore {
    pub(super) fn select_owner(&mut self, now: f64) -> (Option<Snapshot>, &'static str) {
        let active: Vec<_> = self
            .snapshots
            .values()
            .filter(|s| s.fresh(now))
            .cloned()
            .collect();
        let current = self.owner.and_then(|o| self.snapshots.get(&o)).cloned();
        if self.owner.is_none() {
            let selected = activation_choice(&active);
            self.owner = selected.as_ref().map(|s| s.source);
            if let Some(selected) = &selected {
                self.owner_epoch_floor = selected.activation_epoch;
                self.owner_seen_activation_revision = self.activation_revision;
            }
            let reason = if selected.is_none() {
                "no_owner"
            } else {
                "activated"
            };
            return (selected, reason);
        }
        if let Some(pending) = self.pending_owner_expiry.take() {
            if let Some(current) = current
                .as_ref()
                .filter(|c| Some(pending.0) == self.owner && c.session_id == pending.1)
            {
                let alternatives: Vec<_> = active
                    .iter()
                    .filter(|s| s.source != current.source)
                    .cloned()
                    .collect();
                let selected = fallback_choice(&alternatives)
                    .or_else(|| current.fresh(now).then(|| current.clone()));
                self.owner = selected.as_ref().map(|s| s.source);
                self.owner_epoch_floor = if selected.is_none() {
                    0
                } else {
                    active.iter().map(|s| s.activation_epoch).max().unwrap_or(0)
                };
                self.owner_seen_activation_revision = self.activation_revision;
                return (selected, "owner_expired");
            }
        }
        let reason = match current.as_ref() {
            None => "owner_missing",
            Some(current) if current.lifecycle.terminal() => "owner_terminal",
            Some(current) if current.lifecycle != Lifecycle::Guiding => "owner_inactive",
            Some(current) if !current.fresh(now) => "owner_expired",
            Some(current) => {
                let new_activations: Vec<_> = active
                    .iter()
                    .filter(|s| {
                        self.source_activation_revisions
                            .get(&s.source)
                            .copied()
                            .unwrap_or(0)
                            > self.owner_seen_activation_revision
                    })
                    .cloned()
                    .collect();
                if let Some(challenger) = activation_choice(&new_activations) {
                    if challenger.activation_epoch > self.owner_epoch_floor
                        || (challenger.activation_epoch == self.owner_epoch_floor
                            && challenger.source < current.source)
                    {
                        self.owner = Some(challenger.source);
                        self.owner_epoch_floor = challenger.activation_epoch;
                        self.owner_seen_activation_revision = self.activation_revision;
                        return (Some(challenger), "new_activation");
                    }
                    self.owner_epoch_floor =
                        self.owner_epoch_floor.max(challenger.activation_epoch);
                    self.owner_seen_activation_revision = self.activation_revision;
                }
                return (Some(current.clone()), "owner_sticky");
            }
        };
        let selected = fallback_choice(&active);
        self.owner = selected.as_ref().map(|s| s.source);
        self.owner_epoch_floor = if selected.is_none() {
            0
        } else {
            active.iter().map(|s| s.activation_epoch).max().unwrap_or(0)
        };
        self.owner_seen_activation_revision = self.activation_revision;
        (selected, reason)
    }
}
