use super::Error;
use openpilot_control_policy::math::{clip, interp};

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Lead {
    pub status: bool,
    pub radar: bool,
    pub radar_track_id: i64,
    pub d_rel: f64,
    pub y_rel: f64,
    pub v_rel: f64,
}

pub fn nearest(leads: &[Lead]) -> Option<&Lead> {
    leads
        .iter()
        .filter(|lead| {
            lead.status
                && lead.d_rel > 0.
                && [lead.d_rel, lead.y_rel, lead.v_rel]
                    .iter()
                    .all(|v| v.is_finite())
        })
        .min_by(|a, b| a.d_rel.total_cmp(&b.d_rel))
}

pub fn lateral(lead: &Lead, path: (&[f64], &[f64])) -> Result<f64, Error> {
    let (x, y) = path;
    let valid = x.len() >= 2
        && x.len() == y.len()
        && x.iter().chain(y.iter()).all(|v| v.is_finite())
        && x.windows(2).all(|pair| pair[1] > pair[0]);
    Ok(-lead.y_rel - if valid { interp(lead.d_rel, x, y)? } else { 0. })
}

#[derive(Default)]
pub struct LeadLateralFilter {
    initialized: bool,
    value: f64,
    target: Option<(bool, i64)>,
    distance: Option<f64>,
}

impl LeadLateralFilter {
    pub fn update(&mut self, leads: &[Lead], path: (&[f64], &[f64])) -> Result<f64, Error> {
        let Some(lead) = nearest(leads) else {
            self.initialized = false;
            self.target = None;
            self.distance = None;
            return Ok(0.);
        };
        let target = (lead.radar, lead.radar_track_id);
        if Some(target) != self.target
            || self
                .distance
                .is_some_and(|distance| (lead.d_rel - distance).abs() > 5.)
        {
            self.initialized = false;
        }
        self.target = Some(target);
        self.distance = Some(lead.d_rel);
        let input = lateral(lead, path)?;
        self.value = if self.initialized {
            (0.01 / (0.4 + 0.01)) * input + (1. - 0.01 / (0.4 + 0.01)) * self.value
        } else {
            input
        };
        self.initialized = true;
        Ok(self.value)
    }
}

pub fn scc_fields(
    leads: &[Lead],
    path: (&[f64], &[f64]),
    hud: Option<f64>,
) -> Result<[f64; 4], Error> {
    match nearest(leads) {
        Some(lead) => Ok([
            clip(lead.d_rel, 0.1, 204.5),
            clip(
                match hud {
                    Some(value) => value,
                    None => lateral(lead, path)?,
                },
                -45.6,
                5.5,
            ),
            clip(lead.v_rel, -170., 239.3),
            if lead.v_rel > 0. { 1. } else { 2. },
        ]),
        None => Ok([204.6, 0., 239.4, 0.]),
    }
}
