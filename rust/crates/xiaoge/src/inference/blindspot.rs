use super::{session::Session, Error, Geometry};
use crate::compute::ComputeMode;
use crate::{
    config::Config,
    nv12::Frame,
    settings::Settings,
    vasm,
    vision::{Blindspot, Detection, Side},
};
use openpilot_opencv_runtime::Dimensions;
use std::path::Path;

pub struct BlindspotModel {
    net: Option<Session>,
    error: String,
    config: Config,
    geometry: Option<Geometry>,
    state: Blindspot,
}

impl BlindspotModel {
    pub fn load(path: &Path, config: Config) -> Self {
        let (net, error) = if path.is_file() {
            match Session::load(path) {
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

    pub fn loaded(&self) -> bool {
        self.net.as_ref().is_some_and(Session::loaded)
    }
    pub fn error(&self) -> &str {
        self.net
            .as_ref()
            .filter(|net| !net.loaded())
            .map_or(&self.error, Session::error)
    }
    pub fn set_compute_mode(&mut self, mode: ComputeMode) {
        if self.net.as_mut().is_some_and(|net| net.set_mode(mode)) {
            self.state = Blindspot::default();
        }
    }
    pub fn compute_status(&self) -> serde_json::Value {
        self.net.as_ref().map_or_else(
            || serde_json::json!({"loaded":false,"error":self.error}),
            Session::status,
        )
    }
    pub fn compute_timing(&self) -> serde_json::Value {
        self.net.as_ref().map_or(serde_json::Value::Null, |net| {
            serde_json::to_value(&net.timing).unwrap_or(serde_json::Value::Null)
        })
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

    fn confidence(&mut self, frame: &Frame<'_>, side: Side, threshold: f64) -> Result<f64, Error> {
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
        let confidence = vasm::confidence(output.shape(), output.values())?;
        if let Some(shadow) = net.take_shadow() {
            let started = std::time::Instant::now();
            let cpu_started = super::session::cpu_seconds();
            match shadow
                .first()
                .ok_or(Error::Contract("comparison output missing"))
                .and_then(|tensor| {
                    Ok(vasm::confidence(
                        tensor.view().shape(),
                        tensor.view().values(),
                    )?)
                }) {
                Ok(cpu) => net.decoded(
                    serde_json::json!({"confidence":cpu,"above_threshold":cpu>=threshold,"smoothing_compared":false}),
                    serde_json::json!({"confidence":confidence,"above_threshold":confidence>=threshold,"smoothing_compared":false}),
                    (cpu>=threshold) == (confidence>=threshold),
                ),
                Err(error) => net.comparison_error(error.to_string()),
            }
            net.add_comparison_cost(
                started.elapsed().as_secs_f64() * 1000.0,
                (super::session::cpu_seconds() - cpu_started) * 1000.0,
            );
        }
        Ok(confidence)
    }

    pub fn update(
        &mut self,
        frame: &Frame<'_>,
        side: Side,
        settings: &Settings,
        dt: f64,
    ) -> Result<(), Error> {
        if let Some(net) = &mut self.net {
            net.begin_frame();
        }
        let confidence = self.confidence(frame, side, settings.threshold)?;
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
