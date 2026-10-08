mod owner;
mod projection;
pub mod safety;
mod types;
use std::collections::{BTreeMap, BTreeSet};
pub use types::*;

#[derive(Default, Debug)]
pub struct SourceStore {
    snapshots: BTreeMap<Source, Snapshot>,
    sequences: BTreeMap<(Source, String), u64>,
    terminal_sessions: BTreeSet<(Source, String)>,
    transport_losses: BTreeMap<(Source, String), f64>,
    owner: Option<Source>,
    owner_epoch_floor: u64,
    owner_seen_activation_revision: u64,
    activation_epoch: u64,
    activation_revision: u64,
    source_activation_revisions: BTreeMap<Source, u64>,
    last_activation_mono_s: Option<f64>,
    pending_owner_expiry: Option<(Source, String)>,
    projection_key: Option<(Source, String, u64, [bool; 13])>,
    projection_revision: u64,
}

impl SourceStore {
    pub fn accept(&mut self, mut candidate: Snapshot, now: f64) -> bool {
        if !now.is_finite()
            || !candidate.received_mono_s.is_finite()
            || candidate.session_id.is_empty()
            || candidate
                .owner_received_mono_s
                .is_some_and(|v| !v.is_finite() || v > now)
        {
            return false;
        }
        let key = (candidate.source, candidate.session_id.clone());
        if self.terminal_sessions.contains(&key)
            || self
                .sequences
                .get(&key)
                .is_some_and(|v| candidate.sequence <= *v)
        {
            return false;
        }
        let current = self.snapshots.get(&candidate.source);
        if current.is_some_and(|v| now < v.received_mono_s) {
            return false;
        }
        if candidate.source == Source::NaverV1 {
            if let Some(current) = current.filter(|c| {
                c.session_id == candidate.session_id
                    && c.lifecycle == Lifecycle::Guiding
                    && candidate.lifecycle == Lifecycle::Guiding
            }) {
                for (incoming, cached) in [
                    (&mut candidate.control.safety, &current.control.safety),
                    (
                        &mut candidate.control.secondary_safety,
                        &current.control.secondary_safety,
                    ),
                ] {
                    if let (Some(incoming), Some(cached)) = (incoming, cached) {
                        let mut value = incoming.clone();
                        value.received_mono_s = cached.received_mono_s;
                        if value == *cached {
                            *incoming = cached.clone();
                        }
                    }
                }
            }
        }
        if !candidate.normalize(now) {
            return false;
        }
        if candidate.lifecycle.terminal()
            && current.is_some_and(|c| c.session_id != candidate.session_id)
        {
            self.sequences.insert(key.clone(), candidate.sequence);
            self.transport_losses.remove(&key);
            self.terminal_sessions.insert(key);
            return true;
        }
        let expired_refresh = self.owner == Some(candidate.source)
            && candidate.source != Source::TmapLegacy
            && current.is_some_and(|c| {
                c.session_id == candidate.session_id
                    && c.lifecycle == Lifecycle::Guiding
                    && candidate.lifecycle == Lifecycle::Guiding
                    && !c.fresh(now)
            });
        let activating = candidate.lifecycle == Lifecycle::Guiding
            && current.is_none_or(|c| {
                c.lifecycle != Lifecycle::Guiding
                    || c.session_id != candidate.session_id
                    || (candidate.source == Source::TmapLegacy && !c.fresh(now))
            });
        candidate.activation_epoch = if activating {
            if self.last_activation_mono_s != Some(now) {
                self.activation_epoch += 1;
                self.last_activation_mono_s = Some(now);
            }
            self.activation_revision += 1;
            self.source_activation_revisions
                .insert(candidate.source, self.activation_revision);
            self.activation_epoch
        } else {
            current.map_or(0, |c| c.activation_epoch)
        };
        candidate.received_mono_s = now;
        self.sequences.insert(key.clone(), candidate.sequence);
        self.transport_losses.remove(&key);
        if expired_refresh {
            self.pending_owner_expiry = Some(key.clone());
        } else if self.pending_owner_expiry.as_ref().is_some_and(|p| {
            p.0 == candidate.source
                && (p.1 != candidate.session_id || candidate.lifecycle != Lifecycle::Guiding)
        }) {
            self.pending_owner_expiry = None;
        }
        if candidate.lifecycle.terminal() {
            self.terminal_sessions.insert(key);
        }
        self.snapshots.insert(candidate.source, candidate);
        true
    }

    pub fn record_transport_loss(&mut self, source: Source, session: &str, now: f64) -> bool {
        if !now.is_finite()
            || self
                .snapshots
                .get(&source)
                .is_none_or(|c| c.session_id != session || now < c.received_mono_s)
        {
            return false;
        }
        self.transport_losses
            .insert((source, session.to_owned()), now);
        true
    }
}

fn age(now: f64, received: f64) -> f64 {
    (now - received).max(0.)
}
