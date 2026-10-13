use super::projection::Data;
use crate::models::{ClusterUiState, ModelPathPoint};
use num_traits::ToPrimitive;

#[derive(Default)]
pub(super) struct Route {
    pub coordinates: Vec<(f64, f64)>,
    path: Vec<ModelPathPoint>,
    anchor: Option<(f64, f64, f64)>,
}
impl Route {
    pub fn update(&mut self, data: Data<'_>) {
        let coordinates = data.field("coordinates").items();
        let count = coordinates.len();
        let selected = (0..count.min(4096)).filter_map(|index| {
            let index = if count > 4096 {
                (index.to_f64()? * (count - 1).to_f64()? / 4095.0)
                    .round_ties_even()
                    .to_usize()?
            } else {
                index
            };
            let point = coordinates.get(index)?;
            let lat = point.field("latitude").float()?;
            let lon = point.field("longitude").float()?;
            ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)).then_some((lat, lon))
        });
        self.coordinates = selected.collect();
        self.path.clear();
        self.anchor = None;
    }
    pub fn apply(&mut self, state: &mut ClusterUiState, man: Data<'_>) {
        let lat = man.number("xPosLat", 0.0);
        let lon = man.number("xPosLon", 0.0);
        if lat.abs() < 1e-6
            || lon.abs() < 1e-6
            || !(-90.0..=90.0).contains(&lat)
            || !(-180.0..=180.0).contains(&lon)
            || self.coordinates.len() < 2
        {
            return;
        }
        let heading = man.number("xPosAngle", 0.0).rem_euclid(360.0);
        let anchor = (lat, lon, heading);
        if self.anchor != Some(anchor) || self.path.is_empty() {
            let radians = heading.to_radians();
            let cos = radians.cos();
            let sin = radians.sin();
            let meters_lat = 40_008_000.0 / 360.0;
            let meters_lon = meters_lat * lat.to_radians().cos();
            self.path = vec![ModelPathPoint::default()];
            let mut previous = 0.0;
            for (latitude, longitude) in &self.coordinates {
                let east = (longitude - lon) * meters_lon;
                let north = (latitude - lat) * meters_lat;
                let lateral = east * cos - north * sin;
                let forward = east * sin + north * cos;
                if !(0.0..=180.0).contains(&forward)
                    || lateral.abs() > 40.0
                    || forward <= previous + 0.25
                {
                    continue;
                }
                self.path.push(ModelPathPoint {
                    forward_m: forward,
                    lateral_m: lateral,
                    ..ModelPathPoint::default()
                });
                previous = forward;
                if self.path.len() >= 96 {
                    break;
                }
            }
            self.anchor = Some(anchor);
        }
        if self.path.len() < 2 {
            return;
        }
        state.model_path.clone_from(&self.path);
        state.planned_speed_kph = None;
        state.planned_accel_mps2 = None;
        state.planned_curvature_m_inv = None;
    }
}
