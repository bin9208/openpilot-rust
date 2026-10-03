use super::{input::Input, points::ModelPoint, projection::Projection};
use crate::{context::Context, params::Read, Error};
use openpilot_cereal::log_capnp::{model_data_v2, x_y_z_t_data};
use openpilot_ui_framework::geometry::{Point, Rect};
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct ModelPoints {
    pub raw: Vec<ModelPoint>,
    pub projected: Vec<Point>,
}
#[derive(Serialize)]
pub struct Common {
    pub longitudinal: bool,
    pub experimental: bool,
    pub path_height: f64,
    pub path: ModelPoints,
    pub lanes: [ModelPoints; 4],
    pub roads: [ModelPoints; 2],
    pub probabilities: Vec<f32>,
    pub deviations: Vec<f32>,
    pub acceleration: Vec<f32>,
    pub projection: Projection,
    pub transform_dirty: bool,
}
impl Common {
    pub fn new(context: &Context) -> Result<Self, Error> {
        let longitudinal =
            if let Some(bytes) = context.params.bytes("CarParams")?.filter(|b| !b.is_empty()) {
                crate::state::CarConfig::decode(&bytes)?.openpilot_longitudinal_control
            } else {
                false
            };
        Ok(Self {
            longitudinal,
            experimental: false,
            path_height: 1.22,
            path: ModelPoints::default(),
            lanes: std::array::from_fn(|_| ModelPoints::default()),
            roads: std::array::from_fn(|_| ModelPoints::default()),
            probabilities: vec![0.; 4],
            deviations: vec![0.; 2],
            acceleration: Vec::new(),
            projection: Projection::default(),
            transform_dirty: true,
        })
    }
    pub fn header(&mut self, input: &Input<'_>, rect: Rect) -> Result<(), Error> {
        self.projection.clip = Rect {
            x: rect.x - 500.,
            y: rect.y - 500.,
            width: rect.width + 1000.,
            height: rect.height + 1000.,
        };
        self.experimental = input.selfdrive.get_experimental_mode();
        let height = input.calibration.get_height()?;
        self.path_height = if height.is_empty() {
            1.22
        } else {
            f64::from(height.get(0))
        };
        if input.updated("carParams")? {
            self.longitudinal = input.params.get_openpilot_longitudinal_control();
        }
        Ok(())
    }
    pub fn raw(&mut self, model: model_data_v2::Reader<'_>) -> Result<(), Error> {
        self.path.raw = read_points(model.get_position()?)?;
        for (index, line) in model.get_lane_lines()?.iter().enumerate() {
            self.lanes
                .get_mut(index)
                .ok_or(Error::Contract("more than four model lanes"))?
                .raw = read_points(line)?;
        }
        for (index, line) in model.get_road_edges()?.iter().enumerate() {
            self.roads
                .get_mut(index)
                .ok_or(Error::Contract("more than two model road edges"))?
                .raw = read_points(line)?;
        }
        self.probabilities = model.get_lane_line_probs()?.iter().collect();
        self.deviations = model.get_road_edge_stds()?.iter().collect();
        self.acceleration = model.get_acceleration()?.get_x()?.iter().collect();
        Ok(())
    }
    pub fn probability(&self, index: usize) -> Result<f32, Error> {
        self.probabilities
            .get(index)
            .copied()
            .ok_or(Error::Contract("missing lane probability"))
    }
    pub fn deviation(&self, index: usize) -> Result<f32, Error> {
        self.deviations
            .get(index)
            .copied()
            .ok_or(Error::Contract("missing road deviation"))
    }
}
pub fn read_points(line: x_y_z_t_data::Reader<'_>) -> Result<Vec<ModelPoint>, Error> {
    let (xs, ys, zs) = (line.get_x()?, line.get_y()?, line.get_z()?);
    if xs.len() != ys.len() || xs.len() != zs.len() {
        return Err(Error::Contract("model XYZ dimensions"));
    }
    Ok((0..xs.len())
        .map(|i| ModelPoint([xs.get(i), ys.get(i), zs.get(i)]))
        .collect())
}
