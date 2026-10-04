use super::{Error, Geometry};
use crate::{
    config::Config,
    nv12::Frame,
    settings::Settings,
    vasm,
    vision::{Blindspot, Detection, Side},
};
use openpilot_opencv_runtime::{Dimensions, DnnNet};
use std::path::Path;

pub struct BlindspotModel {
    net: Option<DnnNet>,
    error: String,
    config: Config,
    geometry: Option<Geometry>,
    state: Blindspot,
}

impl BlindspotModel {
    pub fn load(path: &Path, config: Config) -> Self {
        let (net, error) = if path.is_file() {
            match DnnNet::load(path) {
                Ok(net) => (Some(net), String::new()),
                Err(error) => (None, format!("could not load model: {error}")),
            }
        } else {
            (None, format!("model missing: {}", path.display()))
        };
        Self {
            net,
            error,
            config,
            geometry: None,
            state: Blindspot::default(),
        }
    }

    pub const fn loaded(&self) -> bool {
        self.net.is_some()
    }
    pub fn error(&self) -> &str {
        &self.error
    }
    pub fn configured(&self, side: Side) -> bool {
        match side {
            Side::Left => self.config.poly_left.len() >= 3,
            Side::Right => self.config.poly_right.len() >= 3,
        }
    }
    pub fn side(&self, side: Side) -> &Detection {
        self.state.side(side)
    }

    pub fn load_config(&mut self, config: Config) {
        self.config = config;
        self.geometry = None;
        self.state = Blindspot::default();
    }

    fn confidence(&mut self, frame: &Frame<'_>, side: Side) -> Result<f64, Error> {
        let layout = frame.layout();
        let dimensions = Dimensions::new(
            u32::try_from(layout.width).map_err(|_| Error::Contract("frame width overflow"))?,
            u32::try_from(layout.height).map_err(|_| Error::Contract("frame height overflow"))?,
        )?;
        if self
            .geometry
            .as_ref()
            .is_none_or(|geometry| geometry.dimensions() != dimensions)
        {
            self.geometry = Some(Geometry::new(&self.config, dimensions)?);
        }
        let Some(net) = &mut self.net else {
            return Ok(0.0);
        };
        let geometry = self
            .geometry
            .as_ref()
            .ok_or(Error::Contract("blindspot geometry missing"))?;
        let Some(tensor) = geometry.tensor(frame, side)? else {
            return Ok(0.0);
        };
        let outputs = net.forward(tensor.view(), &[])?;
        let output = outputs
            .first()
            .ok_or(Error::Contract("blindspot model output missing"))?
            .view();
        Ok(vasm::confidence(output.shape(), output.values())?)
    }

    pub fn update(
        &mut self,
        frame: &Frame<'_>,
        side: Side,
        settings: &Settings,
        dt: f64,
    ) -> Result<(), Error> {
        let confidence = self.confidence(frame, side)?;
        self.state.update(
            side,
            confidence,
            settings.threshold,
            settings.smoothing_seconds,
            dt,
        );
        Ok(())
    }
}
