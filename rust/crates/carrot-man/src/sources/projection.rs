use super::{age, Instruction, Selection, SourceStore};

impl SourceStore {
    pub fn select(&mut self, now: f64) -> Result<Selection, &'static str> {
        if !now.is_finite() {
            return Err("now_s must be a finite local monotonic time");
        }
        let (raw, reason) = self.select_owner(now);
        let mut selection = Selection {
            snapshot: None,
            reason,
            owner_age_s: None,
            safety_age_s: None,
            road_category_age_s: None,
            projection_revision: 0,
            transport_loss_age_s: None,
            secondary_safety_age_s: None,
        };
        let key = if let Some(mut raw) = raw {
            let lease = raw.source.lease();
            selection.owner_age_s = Some(age(now, raw.owner_receipt()));
            let c = &mut raw.control;
            selection.safety_age_s = c.safety.as_ref().map(|s| age(now, s.received_mono_s));
            selection.secondary_safety_age_s = c
                .secondary_safety
                .as_ref()
                .map(|s| age(now, s.received_mono_s));
            selection.road_category_age_s = c.road_category_received_mono_s.map(|t| age(now, t));
            let fresh = |receipt: Option<f64>| receipt.is_some_and(|t| age(now, t) < lease);
            let current = c.current.present && fresh(c.current.received_mono_s);
            let next = c.next.present && fresh(c.next.received_mono_s);
            let safety = selection.safety_age_s.is_some_and(|a| a < lease);
            let secondary = selection.secondary_safety_age_s.is_some_and(|a| a < lease);
            let speed = c.speed_present && fresh(c.speed_received_mono_s);
            let road = c.road_limit_kph.is_some() && fresh(c.road_limit_received_mono_s);
            let category = c.road_category.is_some() && fresh(c.road_category_received_mono_s);
            let route = c.route_present && fresh(c.route_received_mono_s);
            let status = c.status_present && fresh(c.status_received_mono_s);
            let destination = c.destination_present
                && c.destination.is_some()
                && fresh(c.destination_received_mono_s);
            let position = c.position_present && fresh(c.position_received_mono_s);
            let traffic = c.traffic_present && fresh(c.traffic_received_mono_s);
            if !current {
                c.current = Instruction::default();
            }
            if !next {
                c.next = Instruction::default();
            }
            if !safety {
                c.safety = None;
            }
            if !secondary {
                c.secondary_safety = None;
            }
            c.speed_present = speed;
            if !speed {
                c.speed_received_mono_s = None;
            }
            if !road {
                c.road_limit_kph = None;
                c.road_limit_received_mono_s = None;
            }
            if !category {
                c.road_category = None;
                c.road_category_received_mono_s = None;
            }
            c.route_present = route;
            if !route {
                c.route_revision = None;
                c.route_received_mono_s = None;
                c.remaining_distance_m = 0.;
                c.remaining_time_s = 0.;
                c.route_points.clear();
            }
            c.status_present = status;
            if !status {
                c.status_received_mono_s = None;
                c.off_route = false;
            }
            c.destination_present = destination;
            if !destination {
                c.destination = None;
                c.destination_received_mono_s = None;
            }
            c.position_present = position;
            if !position {
                c.position_received_mono_s = None;
            }
            c.traffic_present = traffic;
            if !traffic {
                c.traffic_received_mono_s = None;
            }
            let flags = [
                current,
                next,
                safety,
                secondary,
                speed,
                road,
                category,
                route,
                status,
                destination,
                position,
                traffic,
                false,
            ];
            let key = (raw.source, raw.session_id.clone(), raw.sequence, flags);
            selection.transport_loss_age_s = self
                .transport_losses
                .get(&(raw.source, raw.session_id.clone()))
                .map(|t| age(now, *t));
            selection.snapshot = Some(raw);
            Some(key)
        } else {
            None
        };
        if key != self.projection_key {
            self.projection_revision += 1;
            self.projection_key = key;
        }
        selection.projection_revision = self.projection_revision;
        Ok(selection)
    }
}
