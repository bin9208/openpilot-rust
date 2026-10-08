use super::{constants::*, range::central_vision, scc, Controller, Input, Output};
use crate::{
    association,
    lead::{self, Entry},
    math::maximum,
    path::Path,
    point::{Identity, Point, TrackId},
    predictor,
    primary::{self, constants::*, source, stationary_geometry::cross_position_cost},
    scope,
    selection::{Candidate, Identity as CandidateIdentity},
    trajectory_cutin, trajectory_cutout, Error, Lead,
};
use std::collections::HashSet;

fn pick_side(leads: &[Lead]) -> Option<Lead> {
    leads
        .iter()
        .filter(|lead| lead.d_rel > 5. && lead.d_path.abs() < 3.5)
        .min_by(|a, b| a.d_rel.total_cmp(&b.d_rel))
        .copied()
}
fn pick_two(leads: &[Lead]) -> Vec<Lead> {
    let mut usable = leads
        .iter()
        .filter(|lead| lead.v_lead > 2. && lead.d_path.abs() < 4.2 && lead.d_rel > 2.);
    let Some(first) = usable.next() else {
        return Vec::new();
    };
    let mut result = vec![*first];
    if let Some(second) = usable.find(|lead| lead.d_rel - first.d_rel >= 5.) {
        result.push(*second);
    }
    result
}
fn candidate(lead: Lead, point: &Point) -> Candidate {
    Candidate {
        lead,
        source: point.source.clone(),
        track_id: point.track_id,
        continuity_id: 0,
        retainable: true,
        confirmed_cutin: false,
        confirmed_stationary_shadow: false,
        allow_low_speed: false,
    }
}

impl<P: crate::predictor::CutOutPredictor> Controller<P> {
    fn display_leads(
        &self,
        scoped: &[scope::Scoped<'_>],
        extra: HashSet<Identity>,
    ) -> Result<[Vec<Lead>; 3], Error> {
        let projections: std::collections::HashMap<_, _> = scoped
            .iter()
            .map(|value| (value.point.identity(), value.projection))
            .collect();
        let mut visible: HashSet<_> = scope::visible(scoped, None, &HashSet::new())
            .into_iter()
            .map(Point::identity)
            .collect();
        visible.extend(extra);
        let mut result = [Vec::new(), Vec::new(), Vec::new()];
        for value in scoped {
            if !visible.contains(&value.point.identity()) {
                continue;
            }
            let d_path = projections[&value.point.identity()].d_path;
            let lead = self.radar_lead(value.point, d_path, 0.03, 0.)?;
            let index = if d_path.abs() < 1.8 {
                1
            } else if d_path > 0. {
                0
            } else {
                2
            };
            result[index].push(lead);
        }
        for values in &mut result {
            values.sort_by(|a, b| a.d_rel.total_cmp(&b.d_rel));
        }
        Ok(result)
    }
    pub fn update(&mut self, input: &Input) -> Result<Output, Error> {
        let Input {
            time_s: time,
            v_ego,
            points: raw_points,
            model,
            yaw_rate_rad_s: yaw,
            radar_to_model_time_s: skew,
        } = input;
        let (time, v_ego, yaw, skew) = (*time, *v_ego, *yaw, *skew);
        let path_points = model.path();
        if path_points.len() < 2 {
            self.reset_invalid_path();
            return Ok(Output::default());
        }
        let path = Path::new(&path_points)?;
        let points = self.points_at_model_time(raw_points, v_ego, skew);
        self.lead_dynamics.update(&points, time - skew);
        let matches = self.front_kinematic_associator.update(&points);
        let selected = source::dpath_primary(&points, self.enable_radar_tracks);
        let stationary = source::dpath_fallback(&points, self.enable_radar_tracks);
        let allowed: HashSet<String> = if self.enable_radar_tracks <= -2 {
            HashSet::new()
        } else if self.enable_radar_tracks <= 0 {
            HashSet::from(["scc".to_owned()])
        } else if self.enable_radar_tracks == 2 {
            HashSet::from(["scc".to_owned(), "frontRadar".to_owned()])
        } else {
            HashSet::from(["frontRadar".to_owned()])
        };
        let mut matched = self.primary_matcher.update(
            &primary::Frame {
                vision: model.primary_vision(),
                points: &selected,
                path: &path,
                time: Some(time),
                prefer_corner: false,
                prefer_primary: true,
                yaw_rate: yaw,
                allowed_output_sources: Some(&allowed),
            },
            Some(&stationary),
        );
        let vision = self.primary_matcher._vision_fallback;
        if self.enable_radar_tracks == -1 {
            matched = source::unconditional_scc(&points);
            self.reset_range_mismatch();
            self._moving_range_last_point = None;
            self._moving_range_last_time_s = None;
            self.reset_stationary_range();
        } else if self.reject_farther(matched.as_ref(), vision, &path, time) {
            matched = None;
        }
        if matched.is_none() && self.enable_radar_tracks == 3 {
            matched = source::unconditional_scc(&points);
        }
        if matched.is_some() {
            self.scc_primary_fallback_matcher.reset();
        } else if self.enable_radar_tracks == 2 {
            let selected: Vec<_> = source::primary_points(&points, 2)
                .into_iter()
                .filter(|point| point.source == "scc")
                .collect();
            let allowed = HashSet::from(["scc".to_owned()]);
            matched = self.scc_primary_fallback_matcher.update(
                &primary::Frame {
                    vision: model.primary_vision(),
                    points: &selected,
                    path: &path,
                    time: Some(time),
                    prefer_corner: false,
                    prefer_primary: true,
                    yaw_rate: yaw,
                    allowed_output_sources: Some(&allowed),
                },
                Some(&[]),
            );
        } else {
            self.scc_primary_fallback_matcher.reset();
        }
        let mut lead_one = if let Some(matched) = &matched {
            Some(self.radar_lead(
                &matched.point,
                matched.d_path,
                matched.probability,
                matched.score,
            )?)
        } else {
            vision
                .filter(|_| {
                    self.enable_radar_tracks <= VISION_ONLY_RADAR_TRACK_MODE
                        || central_vision(vision, &path)
                })
                .map(|vision| lead::from_vision(&vision, &path, [v_ego, model.ego_speed(v_ego)]))
        };
        let cutout_point = matched
            .as_ref()
            .filter(|_| self.enable_radar_tracks > 0)
            .map(|matched| &matched.point);
        let paired: Vec<_> = points
            .iter()
            .filter(|point| {
                cutout_point.is_some_and(|cutout| {
                    point.corner()
                        && matches
                            .get(&point.identity())
                            .is_some_and(|front| front.track_id == cutout.track_id)
                        && (point.d_rel - cutout.d_rel).abs() <= 1.5
                        && (point.v_rel - cutout.v_rel).abs() <= 2.
                })
            })
            .collect();
        let lateral = cutout_point.and_then(|cutout| {
            paired
                .into_iter()
                .min_by(|a, b| {
                    (a.d_rel - cutout.d_rel)
                        .abs()
                        .total_cmp(&(b.d_rel - cutout.d_rel).abs())
                })
                .or(Some(cutout))
        });
        let cutout = self.trajectory_cutout.update(trajectory_cutout::Input {
            time_s: time,
            point: cutout_point,
            lateral,
            vision: vision.as_ref(),
            path: Some(&path),
            v_ego,
            yaw_rate: yaw,
        })?;
        if let Some(lead) = &mut lead_one {
            lead.cut_out_time = cutout.time_s;
            lead.cut_out_confidence = cutout.confidence;
        }
        let mut motion = self.select_motion_points(&points);
        if self.motion_sensor == "corner" {
            let matched_front: HashSet<_> = matches.values().map(Point::identity).collect();
            motion.extend(
                points
                    .iter()
                    .filter(|point| {
                        point.source == "frontRadar"
                            && point.v_rel.abs() <= 5.
                            && !matched_front.contains(&point.identity())
                    })
                    .cloned(),
            );
        }
        let scoped = scope::points(&motion, &path);
        let estimates = self
            .trajectory_cutin
            .update(trajectory_cutin::Frame {
                time_s: time,
                v_ego,
                points: &motion,
                path: &path,
                model,
                yaw_rate: yaw,
                vision_required_front: self.motion_sensor == "corner",
                primary: lead_one.as_ref(),
                matches: Some(&matches),
            })?
            .to_vec();
        let estimate_ids = estimates
            .iter()
            .map(|estimate| estimate.point.identity())
            .collect();
        let [leads_left, leads_center, leads_right] = self.display_leads(&scoped, estimate_ids)?;
        let active = self.lead_two_tracker.active_identity.clone();
        let mut candidates = Vec::new();
        let mut confirmed_leads = Vec::new();
        let mut risk_leads = Vec::new();
        self._same_row_suppressed_until
            .retain(|_, until| time <= *until);
        for estimate in &estimates {
            let point = &estimate.point;
            let lead_point = association::prefer_front(point, &matches);
            let d_path = if point.corner() && matches.contains_key(&point.identity()) {
                path.project(lead_point.d_rel, lead_point.y_rel).d_path
            } else {
                estimate.d_path
            };
            let lead = self.radar_lead(&lead_point, d_path, 0.03, estimate.confidence)?;
            let source = if estimate.cross_sensor_supported {
                lead_point
                    .kinematics_source
                    .clone()
                    .unwrap_or_else(|| point.source.clone())
            } else {
                point.source.clone()
            };
            let track_id = if estimate.cross_sensor_supported {
                lead_point.kinematics_track_id.unwrap_or(point.track_id)
            } else {
                point.track_id
            };
            let continuity_id = if estimate.cross_sensor_supported {
                u64::try_from(track_id.0)
                    .map_err(|_| Error::Contract("negative cross-sensor continuity identity"))?
            } else {
                estimate.continuity_id
            };
            let identity = CandidateIdentity(source.clone(), track_id, continuity_id);
            let same_row = estimate.cross_sensor_supported
                && !estimate.vision_supported
                && lead_one.is_some_and(|primary| {
                    primary.status && lead.d_rel > 8. && (lead.d_rel - primary.d_rel).abs() <= 3.
                });
            if same_row {
                self._same_row_suppressed_until
                    .insert(identity.clone(), time + 0.75);
            }
            let suppressed = !estimate.vision_supported
                && time
                    <= self
                        ._same_row_suppressed_until
                        .get(&identity)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY);
            if lead::duplicates_primary(&lead, lead_one.as_ref()) || suppressed {
                if active.as_ref() == Some(&identity) {
                    self.lead_two_tracker.reset();
                }
                continue;
            }
            let compete = lead::can_compete(
                &lead,
                lead_one.as_ref(),
                Entry {
                    projected: estimate.time_to_overlap_s.is_some() || estimate.confirmed_cutin,
                    horizon: estimate.time_to_overlap_s,
                },
            );
            let detected = self.cut_in_sensitivity > 0 && estimate.confirmed_cutin && compete;
            if detected {
                confirmed_leads.push(lead);
            }
            if self.cut_in_sensitivity > 0 && estimate.predecel_risk && compete {
                risk_leads.push(lead);
            }
            if estimate.entry_withdrawn
                || estimate.passing_before_overlap
                || estimate.parallel_drift
                || estimate.rear_pass
                || estimate.stationary_pair_alias
            {
                if active.as_ref() == Some(&identity) {
                    self.lead_two_tracker.reset();
                }
                continue;
            }
            candidates.push(Candidate {
                lead,
                source,
                track_id,
                continuity_id,
                retainable: estimate.current_path || estimate.d_path * estimate.d_path_rate <= 0.,
                confirmed_cutin: detected && estimate.control_eligible,
                allow_low_speed: estimate.cross_sensor_supported,
                confirmed_stationary_shadow: false,
            });
        }
        let lead_cutin_risk = risk_leads.into_iter().min_by(|a, b| {
            (a.d_rel, -a.score)
                .partial_cmp(&(b.d_rel, -b.score))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let front_motion = if self.motion_sensor == "front" {
            motion.clone()
        } else {
            points
                .iter()
                .filter(|point| point.source == "frontRadar")
                .cloned()
                .collect()
        };
        let front_scoped = if self.motion_sensor == "front" {
            scoped.clone()
        } else {
            scope::points(&front_motion, &path)
        };
        let primary_id = lead_one
            .filter(|lead| lead.radar)
            .map_or(TrackId(-1), |lead| TrackId(i128::from(lead.radar_track_id)));
        let requested: HashSet<_> = front_motion
            .iter()
            .filter(|point| point.track_id == primary_id)
            .map(Point::identity)
            .collect();
        let predictions = self.primary_cut_out_predictor.predict(predictor::Frame {
            time_s: time,
            v_ego,
            yaw_rate: yaw,
            points: &front_motion,
            path: &path,
            requested: &requested,
            scoped: Some(&front_scoped),
        })?;
        let probability = predictions
            .iter()
            .filter(|(identity, _)| identity.1 == primary_id)
            .fold(0., |value, (_, prob)| maximum(value, *prob));
        let mut stationary_candidates = Vec::new();
        for value in &scoped {
            if !value.point.corner() {
                continue;
            }
            let probability = self
                .primary_matcher
                ._vision_fallback
                .filter(|vision| {
                    (value.point.v_lead - vision.velocity).abs()
                        <= STATIONARY_MAX_VISION_SPEED_DELTA_MPS
                        && cross_position_cost(Some(*vision), value.point).is_some()
                })
                .map_or(0., |vision| vision.probability);
            let lead = self.radar_lead(value.point, value.projection.d_path, probability, 0.)?;
            stationary_candidates.push(candidate(lead, value.point));
        }
        let handoff = self.stationary_primary_handoff_tracker.update(
            time,
            lead_one.as_ref(),
            &stationary_candidates,
            active.as_ref(),
        );
        if let Some(handoff) = handoff {
            if !candidates
                .iter()
                .any(|candidate| candidate.identity() == handoff.identity())
            {
                candidates.push(handoff);
            }
        }
        let mut shadow_inputs = Vec::new();
        for value in &front_scoped {
            let point = value.point;
            let identity = CandidateIdentity(point.source.clone(), point.track_id, 0);
            let retained = active.as_ref() == Some(&identity);
            let supported = scc::shadow_supported(point, &points, &path);
            if point.radar_track_state < 2 || !(supported || retained) {
                continue;
            }
            let lead = self.radar_lead(point, value.projection.d_path, 0.03, probability)?;
            let candidate = candidate(lead, point);
            if supported {
                shadow_inputs.push(candidate.clone());
            }
            if retained && point.track_id != primary_id {
                candidates.push(candidate);
            }
        }
        let shadow = self.stationary_shadow_tracker.update(
            time,
            lead_one.as_ref(),
            probability,
            &shadow_inputs,
        );
        if let Some(shadow) = shadow {
            if shadow.confirmed_stationary_shadow
                && shadow.track_id != primary_id
                && !candidates
                    .iter()
                    .any(|candidate| candidate.identity() == shadow.identity())
            {
                candidates.push(shadow);
            }
        }
        if let Some(active) = &active {
            if !candidates
                .iter()
                .any(|candidate| candidate.identity() == *active)
            {
                if let Some(point) = points
                    .iter()
                    .find(|point| point.source == active.0 && point.track_id == active.1)
                {
                    let lead_point = association::prefer_front(point, &matches);
                    let d_path = path.project(lead_point.d_rel, lead_point.y_rel).d_path;
                    let lead = self.radar_lead(&lead_point, d_path, 0.03, 0.)?;
                    if !lead::duplicates_primary(&lead, lead_one.as_ref()) {
                        candidates.push(Candidate {
                            continuity_id: active.2,
                            ..candidate(lead, point)
                        });
                    }
                }
            }
        }
        let selection = self
            .lead_two_tracker
            .update(time, lead_one.as_ref(), &candidates);
        let scc_point = points
            .iter()
            .filter(|point| {
                self.enable_radar_tracks >= 2
                    && point.measured
                    && point.source == "scc"
                    && 0.8 < point.d_rel
                    && point.d_rel <= SCC_LEAD_TWO_MAX_DREL_M
                    && point.v_lead < SCC_LEAD_TWO_MAX_VLEAD_MPS
            })
            .min_by(|a, b| a.d_rel.total_cmp(&b.d_rel));
        let physical =
            scc_point.and_then(|scc| scc::physical_support(scc, points.iter(), &path, true));
        let supported = scc_point.is_some_and(|scc| {
            scc::independently_supported(
                scc,
                physical,
                &points,
                &path,
                model.primary_vision(),
                lead_one.as_ref(),
            )
        });
        let scc_point = self.scc_lead_two_tracker.update(time, scc_point, supported);
        let mut scc_lead = None;
        if let Some(scc) = &scc_point {
            let point = physical.unwrap_or(scc);
            let lead = self.radar_lead(
                point,
                path.project(point.d_rel, point.y_rel).d_path,
                0.03,
                if physical.is_some() { 1. } else { 0.5 },
            )?;
            if scc::can_compete(&lead, lead_one.as_ref()) {
                scc_lead = Some(lead);
            }
        }
        let mut lead_two = selection.lead_two;
        if scc_lead.is_some_and(|scc| lead_two.is_none_or(|lead| scc.d_rel < lead.d_rel)) {
            lead_two = scc_lead;
        }
        confirmed_leads.sort_by(|a, b| a.d_rel.total_cmp(&b.d_rel));
        Ok(Output {
            lead_one,
            lead_two,
            lead_left: pick_side(&leads_left),
            lead_right: pick_side(&leads_right),
            leads_left2: pick_two(&leads_left),
            leads_right2: pick_two(&leads_right),
            leads_left,
            leads_center,
            leads_right,
            leads_cutin: confirmed_leads,
            lead_cutin_risk,
        })
    }
}
