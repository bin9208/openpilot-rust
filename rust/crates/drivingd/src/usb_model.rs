use crate::Error;
use openpilot_modeld::{inputs::PolicyInputs, parse::RawOutputs, prediction::DrivingPrediction};
use openpilot_usbgpu::client::{Client, Frame, Launch};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub desire: i32,
    pub is_rhd: bool,
    pub lateral_time: f64,
    pub longitudinal_time: f64,
}
impl Controls {
    pub fn apply(self, inputs: &mut PolicyInputs) {
        inputs.update(
            self.desire,
            self.is_rhd,
            self.lateral_time,
            self.longitudinal_time,
        );
    }
}

pub struct UsbModel {
    pub client: Client,
    pub controls: Controls,
    inputs: PolicyInputs,
    slices: BTreeMap<String, [usize; 2]>,
}
impl UsbModel {
    pub fn load(worker: &Path, model: &Path, camera: [u32; 2]) -> Result<Self, Error> {
        let client = Client::load(worker, model, camera)?;
        Self::from_client(client)
    }
    pub fn launch(launch: Launch<'_>) -> Result<Self, Error> {
        Self::from_client(Client::launch(launch)?)
    }
    pub fn launch_with_assets(
        launch: Launch<'_>,
        binding: &openpilot_usbgpu::worker_artifact::Binding,
    ) -> Result<Self, Error> {
        Self::from_client(Client::launch_with_assets(launch, binding)?)
    }
    pub fn from_client(client: Client) -> Result<Self, Error> {
        let mut slices = BTreeMap::new();
        for (name, section) in &client.info.output_slices {
            let [Some(start), Some(stop), None | Some(1)] = section else {
                return Err(Error::Contract("unsupported USB model output slice"));
            };
            let stop = if name == "action" && stop - start == 4 {
                start + 2
            } else {
                *stop
            };
            slices.insert(name.clone(), [*start, stop]);
        }
        Ok(Self {
            client,
            controls: Controls::default(),
            inputs: PolicyInputs::new(
                slices
                    .get("hidden_state")
                    .map_or(1, |section| section[1] - section[0]),
            )?,
            slices,
        })
    }
    pub fn update(&mut self, controls: Controls) {
        self.controls = controls;
        controls.apply(&mut self.inputs);
    }
    pub fn infer(
        &mut self,
        main: &[u8],
        extra: &[u8],
        transforms: [[f32; 9]; 2],
    ) -> Result<DrivingPrediction, Error> {
        let bytes = self.client.info.frame_size;
        let main = main
            .get(..bytes)
            .ok_or(Error::Contract("USB main NV12 planes are truncated"))?;
        let extra = extra
            .get(..bytes)
            .ok_or(Error::Contract("USB extra NV12 planes are truncated"))?;
        let values = self.client.run(Frame {
            main,
            extra,
            transforms,
            inputs: self.inputs.packed(),
        })?;
        Ok(DrivingPrediction::parse(&RawOutputs::new(
            values,
            &self.slices,
        )?)?)
    }
}
