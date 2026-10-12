use super::profile::ModelProfile;
use super::{
    cache::{cache_grid_value, model_line_cache_end_m, model_line_cache_start_m, Cache},
    dashes::{dashed_lane_start_cursor_m, LANE_DASH_GAP_M, LANE_DASH_LENGTH_M},
    geometry::{extend_model_centerline_rearward, lane_centerline, LaneSpan},
    lines::{model_line_centerline, ModelSpan},
    strips::{
        lane_marking_strip_groups_from_centerline, lane_marking_strip_groups_from_segments,
        strips_from_centerline_width_specs, MarkingSpec, MeshStripGroups, WidthSpec,
    },
    MeshStrip, Vec3,
};
use crate::models::ModelPathPoint;
use std::time::Instant;

type GeometrySpecs = Vec<(i64, f64)>;
type PointKey = Vec<(f64, f64)>;
type ModelKey = (PointKey, f64, f64, f64, String, bool, GeometrySpecs);
type LaneKey = (f64, f64, f64, f64, f64, String, GeometrySpecs);

pub struct GeometryCache {
    model: Cache<ModelKey, Option<MeshStripGroups>>,
    lane: Cache<LaneKey, MeshStripGroups>,
    planned: Cache<(Vec<Vec3>, Vec<WidthSpec>), Vec<MeshStrip>>,
}
impl Default for GeometryCache {
    fn default() -> Self {
        Self {
            model: Cache::new(48),
            lane: Cache::new(64),
            planned: Cache::new(48),
        }
    }
}

#[must_use]
pub fn model_line_geometry_specs(specs: &[MarkingSpec]) -> GeometrySpecs {
    specs
        .iter()
        .map(|(width, _, height)| (*width, *height))
        .collect()
}

#[must_use]
pub fn model_line_placeholder_specs(specs: &[(i64, f64)]) -> Vec<MarkingSpec> {
    specs
        .iter()
        .map(|(width, height)| (*width, [0; 4], *height))
        .collect()
}

#[must_use]
pub fn model_line_cache_point_key(points: &[ModelPathPoint]) -> PointKey {
    points
        .iter()
        .map(|point| {
            (
                (point.forward_m * 20.0).round_ties_even(),
                (point.lateral_m * 20.0).round_ties_even(),
            )
        })
        .collect()
}

impl GeometryCache {
    #[must_use]
    pub fn planned_strips(&mut self, points: &[Vec3], specs: &[WidthSpec]) -> Vec<MeshStrip> {
        let key = (points.to_vec(), specs.to_vec());
        if let Some(strips) = self.planned.get(&key) {
            return strips;
        }
        let strips = strips_from_centerline_width_specs(points, specs);
        self.planned.insert(key, strips.clone());
        strips
    }

    #[must_use]
    pub(super) fn model_line_groups(
        &mut self,
        points: &[ModelPathPoint],
        span: &ModelSpan,
        specs: &[MarkingSpec],
        style: &str,
        profile: &mut ModelProfile<'_, '_>,
    ) -> Option<MeshStripGroups> {
        let scale = 0.001_f64.max(span.scale);
        let start = model_line_cache_start_m(span.start);
        let end = model_line_cache_end_m(span.end);
        let geometry = model_line_geometry_specs(specs);
        let started = Instant::now();
        let point_key = model_line_cache_point_key(points);
        profile.record("key", started);
        let key = (
            point_key,
            start,
            end,
            scale,
            style.into(),
            span.extend_before_model,
            geometry.clone(),
        );
        if let Some(groups) = self.model.get_if(&key, Option::is_some) {
            profile.count("hit");
            return groups;
        }
        profile.count("miss");
        let started = Instant::now();
        let mut centerline = model_line_centerline(points, start, end, 0.0, 0.0, scale);
        profile.record("centerline", started);
        let groups = if centerline.len() < 2 {
            if span.extend_before_model {
                None
            } else {
                Some(vec![Vec::new(); geometry.len()].into())
            }
        } else {
            if span.extend_before_model {
                let started = Instant::now();
                centerline = extend_model_centerline_rearward(&centerline, start);
                profile.record("extend", started);
            }
            let started = Instant::now();
            let groups = lane_marking_strip_groups_from_centerline(
                &centerline,
                &model_line_placeholder_specs(&geometry),
                style,
            );
            profile.record("strips", started);
            Some(groups)
        };
        self.model.insert(key, groups.clone());
        groups
    }

    #[must_use]
    pub fn lane_offset_groups(
        &mut self,
        offset: f64,
        span: &LaneSpan,
        specs: &[MarkingSpec],
        style: &str,
    ) -> MeshStripGroups {
        let offset = cache_grid_value(offset, 0.01);
        let steering = cache_grid_value(span.steering, 0.002);
        let width = 0.1_f64.max(cache_grid_value(span.lane_width, 0.02));
        let start = model_line_cache_start_m(span.start);
        let end = model_line_cache_end_m(span.end);
        let geometry = model_line_geometry_specs(specs);
        let key = (
            offset,
            steering,
            width,
            start,
            end,
            style.into(),
            geometry.clone(),
        );
        if let Some(groups) = self.lane.get(&key) {
            return groups;
        }
        let mut segments = Vec::new();
        if style == "solid" {
            segments.push(lane_centerline(
                offset, steering, width, start, end, 56, 0.0,
            ));
        } else {
            let mut cursor = dashed_lane_start_cursor_m(start, LANE_DASH_LENGTH_M, LANE_DASH_GAP_M);
            while cursor < end {
                let dash_start = cursor.max(start);
                let dash_end = (cursor + LANE_DASH_LENGTH_M).min(end);
                if dash_end > dash_start + 0.001 {
                    segments.push(lane_centerline(
                        offset, steering, width, dash_start, dash_end, 6, 0.0,
                    ));
                }
                cursor += LANE_DASH_LENGTH_M + LANE_DASH_GAP_M;
            }
        }
        let groups = lane_marking_strip_groups_from_segments(
            &segments,
            &model_line_placeholder_specs(&geometry),
        );
        self.lane.insert(key, groups.clone());
        groups
    }
}
