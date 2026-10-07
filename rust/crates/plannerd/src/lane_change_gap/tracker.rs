use super::{GapLead, Plan, Reason};
use crate::{lead::Lead, Error};
use std::collections::{BTreeMap, VecDeque};
mod path;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct Input {
    pub now: f64,
    pub direction: i32,
    pub speed: f64,
    pub yaw_rate: f64,
    pub path_t: Vec<f64>,
    pub path_x: Vec<f64>,
    pub path_y: Vec<f64>,
    pub primary: Option<Lead>,
    pub secondary: Option<Lead>,
    pub blindspot: bool,
    pub valid: bool,
    pub legacy_side_input: bool,
}

#[derive(Debug)]
pub struct Tracker {
    direction: i32,
    started: f64,
    last_time: f64,
    heading: f64,
    ego_y: f64,
    primary_id: i32,
    history: VecDeque<(f64, f64, f64)>,
    target_ids: Vec<i32>,
    targets_since: f64,
    previous_leads: BTreeMap<i32, GapLead>,
    entry_ids: Vec<i32>,
    selection_changed: bool,
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            direction: 0,
            started: 0.,
            last_time: 0.,
            heading: 0.,
            ego_y: 0.,
            primary_id: -1,
            history: VecDeque::new(),
            target_ids: Vec::new(),
            targets_since: 0.,
            previous_leads: BTreeMap::new(),
            entry_ids: Vec::new(),
            selection_changed: false,
        }
    }
}

impl Tracker {
    pub const fn direction(&self) -> i32 {
        self.direction
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn update(&mut self, input: &Input) -> Result<Plan, Error> {
        let active = matches!(input.direction, -1 | 1);
        if input.legacy_side_input || !active || !input.valid || !input.now.is_finite() {
            self.reset();
            let reason = if input.legacy_side_input {
                Reason::LegacySideInput
            } else if active {
                Reason::InvalidInput
            } else {
                Reason::Inactive
            };
            return Ok(Plan {
                active,
                reason,
                ..Plan::default()
            });
        }
        let lead = GapLead::read(input.primary.as_ref());
        let second = GapLead::read(input.secondary.as_ref());
        let targets: Vec<_> = second
            .filter(|lead| lead.distance > 0.)
            .into_iter()
            .collect();
        let invalid_target = input.secondary.is_some_and(|lead| lead.status) && second.is_none();
        let selected_ids: Vec<_> = [lead, second]
            .iter()
            .map(|lead| lead.map_or(-1, |lead| lead.id))
            .collect();
        if self.direction != input.direction {
            self.reset();
            self.direction = input.direction;
            self.started = input.now;
            self.last_time = input.now;
            self.primary_id = lead.map_or(-1, |lead| lead.id);
            self.entry_ids.clone_from(&selected_ids);
        } else if !(0. < input.now - self.last_time && input.now - self.last_time <= 0.15) {
            self.selection_changed = true;
        }
        self.selection_changed |= selected_ids != self.entry_ids;
        let base = Plan {
            active: true,
            primary_id: self.primary_id,
            targets,
            entry_ids: self.entry_ids.clone(),
            selected_ids,
            ..Plan::default()
        };
        if ![input.speed, input.yaw_rate].iter().all(|v| v.is_finite())
            || !(5. ..=35.).contains(&input.speed)
            || input.yaw_rate.abs() > 0.08
        {
            self.history.clear();
            self.selection_changed = true;
            self.last_time = input.now;
            return Ok(Plan {
                reason: Reason::PoseOrSpeed,
                ..base
            });
        }
        if self.selection_changed {
            self.history.clear();
            self.last_time = input.now;
            return Ok(Plan {
                reason: Reason::SelectedLeadsChanged,
                ..base
            });
        }
        let dt = input.now - self.last_time;
        self.ego_y += input.speed * (self.heading + input.yaw_rate * dt * 0.5).sin() * dt;
        self.heading += input.yaw_rate * dt;
        self.last_time = input.now;
        let current: BTreeMap<_, _> = lead
            .iter()
            .chain(base.targets.iter())
            .map(|lead| (lead.id, *lead))
            .collect();
        let discontinuous = current.iter().any(|(id, lead)| {
            self.previous_leads.get(id).is_some_and(|old| {
                (lead.distance - old.distance - old.relative_speed * dt).abs() > 2.
                    || (lead.speed - old.speed).abs() > 3.
                    || (lead.lateral - old.lateral).abs() > 0.8
            })
        });
        self.previous_leads = current;
        if discontinuous {
            self.history.clear();
            self.targets_since = input.now;
            return Ok(Plan {
                reason: Reason::TrackDiscontinuity,
                ..base
            });
        }
        if input.now - self.started > 6. || self.heading.abs() > 0.15 {
            return Ok(Plan {
                reason: Reason::SessionLimit,
                ..base
            });
        }
        let Some(lead) = lead
            .filter(|lead| lead.id == self.primary_id && 8. < lead.distance && lead.distance < 60.)
        else {
            self.history.clear();
            return Ok(Plan {
                reason: Reason::PrimaryChanged,
                ..base
            });
        };
        if input.blindspot
            || invalid_target
            || second.is_some_and(|lead| lead.distance <= 3.)
            || base.targets.is_empty()
            || base.targets.iter().any(|target| target.id == lead.id)
        {
            self.targets_since = input.now;
            return Ok(Plan {
                reason: Reason::DestinationUnconfirmed,
                ..base
            });
        }
        let mut ids: Vec<_> = base.targets.iter().map(|lead| lead.id).collect();
        ids.sort_unstable();
        if ids != self.target_ids {
            self.targets_since = input.now;
            self.target_ids = ids;
        }
        self.check_path(input, lead, base)
    }
}
