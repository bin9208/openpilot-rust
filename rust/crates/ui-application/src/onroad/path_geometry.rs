//! Original two-stage Carrot ribbon interpolation and Float64 projection.
use super::model_renderer::{
    math::{float, interp},
    points::{ModelPoint, SamplePoint},
    projection::{ribbon_vertices, Projection},
};
use crate::Error;
use num_traits::ToPrimitive;
use openpilot_ui_framework::geometry::Point;

pub fn sample_path(line: &[ModelPoint], distances: &[f64]) -> Result<Vec<SamplePoint>, Error> {
    let mut maximum = f32::NEG_INFINITY;
    let nodes: Vec<_> = line
        .iter()
        .map(|p| {
            maximum = if p.0[0].is_nan() || maximum.is_nan() {
                f32::NAN
            } else {
                maximum.max(p.0[0])
            };
            f64::from(maximum)
        })
        .collect();
    let indices: Vec<_> = (0..line.len())
        .map(|index| index.to_f64().ok_or(Error::Contract("path index range")))
        .collect::<Result<_, _>>()?;
    let ys: Vec<_> = line.iter().map(|p| f64::from(p.0[1])).collect();
    let zs: Vec<_> = line.iter().map(|p| f64::from(p.0[2])).collect();
    distances
        .iter()
        .map(|&distance| {
            let index = interp(distance, &nodes, &indices)?;
            Ok(SamplePoint([
                distance,
                interp(index, &indices, &ys)?,
                interp(index, &indices, &zs)?,
            ]))
        })
        .collect()
}
#[derive(Clone, Copy)]
pub struct PathRibbon {
    pub width: f64,
    pub height: [f64; 2],
    pub allow_invert: bool,
}
pub fn project_path(
    projection: &Projection,
    line: &[SamplePoint],
    ribbon: PathRibbon,
) -> Result<Vec<Point>, Error> {
    let mut pairs = Vec::with_capacity(line.len());
    let mut lowest = f64::INFINITY;
    for &SamplePoint([x, y, z]) in line {
        let height = interp(x, &[0., 100.], &ribbon.height)?;
        let width = interp(height, &[-3., 0., 3.], &[1.5, 0.5, 1.5])? * ribbon.width;
        let left = projection.batch_double(SamplePoint([x, y - width, z + height]));
        let right = projection.batch_double(SamplePoint([x, y + width, z + height]));
        if let Some(pair) = projection.pair_double(left, right) {
            if ribbon.allow_invert || pair[0].0[1] <= lowest {
                lowest = pair[0].0[1];
                pairs.push(pair.map(|p| Point {
                    x: float(p.0[0]),
                    y: float(p.0[1]),
                }));
            }
        }
    }
    Ok(ribbon_vertices(&pairs))
}
